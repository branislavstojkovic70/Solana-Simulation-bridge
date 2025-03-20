import { Buffer } from 'buffer';
import fs from 'fs';
import path from 'path';
import {
  AccountLayout,
  createMint,
  createInitializeAccountInstruction,
  getOrCreateAssociatedTokenAccount,
  mintTo,
  TOKEN_PROGRAM_ID,
} from '@solana/spl-token';
import {
  Connection,
  Keypair,
  PublicKey,
  SystemProgram,
  Transaction,
  sendAndConfirmTransaction,
  SYSVAR_RENT_PUBKEY,
} from '@solana/web3.js';

const ESCROW_PROGRAM_ID = new PublicKey('6DhVg5JpDGjKZLUa3Cd9f1rGWTKtnJZFCwxfCTJ3Dgtz');
const LOGGER_PROGRAM_ID = new PublicKey('CsacSZo3y5pcQmLogQ5ChjNx9tBo4jrMziV1GrQFfmh9');
const LOGGER_STATE_FILE = "logger_state.json"; 

const payerKeypair = Keypair.fromSecretKey(
  Uint8Array.from(
    JSON.parse(fs.readFileSync(path.resolve('wallet.json'), 'utf-8')))
);

const connection = new Connection('http://127.0.0.1:8899', 'confirmed');
const ESCROW_ACCOUNT_SIZE = 105;

async function getOrCreateLoggerStateAccount(): Promise<Keypair> {
  if (fs.existsSync(LOGGER_STATE_FILE)) {
    console.log("✅ Logger state account found. Loading...");
    const secretKey = Uint8Array.from(JSON.parse(fs.readFileSync(LOGGER_STATE_FILE, "utf-8")));
    return Keypair.fromSecretKey(secretKey);
  }

  console.log("🚀 Logger state account not found. Creating a new one...");
  const loggerStateAccount = Keypair.generate();

  const createLoggerStateIx = SystemProgram.createAccount({
    fromPubkey: payerKeypair.publicKey,
    newAccountPubkey: loggerStateAccount.publicKey,
    lamports: await connection.getMinimumBalanceForRentExemption(8),
    space: 8,
    programId: LOGGER_PROGRAM_ID,
  });

  const transaction = new Transaction().add(createLoggerStateIx);
  await sendAndConfirmTransaction(connection, transaction, [payerKeypair, loggerStateAccount]);

  console.log("✅ Logger state account created:", loggerStateAccount.publicKey.toBase58());

  fs.writeFileSync(LOGGER_STATE_FILE, JSON.stringify(Array.from(loggerStateAccount.secretKey)));

  return loggerStateAccount;
}

async function main() {
  console.log('Starting test...');

  const loggerStateAccount = await getOrCreateLoggerStateAccount();

  // 1) Create Mint
  const mint = await createMint(
    connection,
    payerKeypair,
    payerKeypair.publicKey,
    null,
    9
  );

  // 2) Associated token accounts
  const initializerTokenAccount = await getOrCreateAssociatedTokenAccount(
    connection,
    payerKeypair,
    mint,
    payerKeypair.publicKey
  );
  const tokenToReceiveAccount = await getOrCreateAssociatedTokenAccount(
    connection,
    payerKeypair,
    mint,
    payerKeypair.publicKey
  );
  const takerTokenAccount = await getOrCreateAssociatedTokenAccount(
    connection,
    payerKeypair,
    mint,
    Keypair.generate().publicKey
  );

  await mintTo(
    connection,
    payerKeypair,
    mint,
    initializerTokenAccount.address,
    payerKeypair,
    100
  );

  console.log('Token mint:', mint.toBase58());
  console.log('Initializer token account:', initializerTokenAccount.address.toBase58());
  console.log('Token to receive account:', tokenToReceiveAccount.address.toBase58());
  console.log('Taker token account:', takerTokenAccount.address.toBase58());

  // 3) Create escrow account
  const escrowAccount = Keypair.generate();
  const createEscrowAccountIx = SystemProgram.createAccount({
    fromPubkey: payerKeypair.publicKey,
    newAccountPubkey: escrowAccount.publicKey,
    lamports: await connection.getMinimumBalanceForRentExemption(ESCROW_ACCOUNT_SIZE),
    space: ESCROW_ACCOUNT_SIZE,
    programId: ESCROW_PROGRAM_ID,
  });

  // 4) Create temp token account
  const tempTokenAccount = Keypair.generate();
  const createTempAccountIx = SystemProgram.createAccount({
    fromPubkey: payerKeypair.publicKey,
    newAccountPubkey: tempTokenAccount.publicKey,
    lamports: await connection.getMinimumBalanceForRentExemption(AccountLayout.span),
    space: AccountLayout.span,
    programId: TOKEN_PROGRAM_ID,
  });

  const initTempAccountIx = createInitializeAccountInstruction(
    tempTokenAccount.publicKey,
    mint,
    payerKeypair.publicKey,
    TOKEN_PROGRAM_ID
  );

  // 5) Build data: [0, <u64>]
  const data = Buffer.alloc(9);
  data.writeUInt8(0, 0); // 0 = InitEscrow
  data.writeBigUInt64LE(BigInt(100), 1);

  // 6) Build instruction
  const initEscrowIx = {
    keys: [
      { pubkey: payerKeypair.publicKey, isSigner: true, isWritable: true },
      { pubkey: tempTokenAccount.publicKey, isSigner: false, isWritable: true },
      { pubkey: tokenToReceiveAccount.address, isSigner: false, isWritable: true },
      { pubkey: escrowAccount.publicKey, isSigner: false, isWritable: true },
      { pubkey: SYSVAR_RENT_PUBKEY, isSigner: false, isWritable: false },
      { pubkey: TOKEN_PROGRAM_ID, isSigner: false, isWritable: false },
      { pubkey: LOGGER_PROGRAM_ID, isSigner: false, isWritable: false },

      { pubkey: loggerStateAccount.publicKey, isSigner: false, isWritable: true },
    ],
    programId: ESCROW_PROGRAM_ID,
    data,
  };

  // 7) Send transaction
  const transaction = new Transaction().add(
    createEscrowAccountIx,
    createTempAccountIx,
    initTempAccountIx,
    initEscrowIx
  );

  const txSig = await sendAndConfirmTransaction(connection, transaction, [
    payerKeypair,
    escrowAccount,
    tempTokenAccount,
  ]);

  console.log('Escrow initialized. Transaction signature:', txSig);

  // 8) Fetch logs
  const txInfo = await connection.getTransaction(txSig, {
    commitment: 'confirmed',
  });
  if (txInfo?.meta) {
    const logs = txInfo.meta.logMessages ?? [];

    console.log('----- ALL LOGS -----');
    logs.forEach((line) => console.log(line));
    console.log('----- END ALL LOGS -----');

    const loggerLines: string[] = [];
    let inLoggerSection = false;
    const loggerProgramStr = `Program ${LOGGER_PROGRAM_ID.toBase58()}`;
    for (const line of logs) {
      if (line.startsWith(loggerProgramStr + " invoke")) {
        inLoggerSection = true;
      }
      if (inLoggerSection) {
        loggerLines.push(line);
      }
      if (line.startsWith(loggerProgramStr + " success")) {
        inLoggerSection = false;
      }
    }

    if (loggerLines.length === 0) {
      console.log('No logger lines found in the logs');
    } else {
      console.log('----- LOGGER LINES -----');
      loggerLines.forEach((l) => console.log(l));
      console.log('----- END LOGGER LINES -----');
    }
  } else {
    console.log('No transaction info/logs found');
  }
}

main().catch(console.error);
