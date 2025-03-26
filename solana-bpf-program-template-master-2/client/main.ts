
import { Buffer } from 'buffer';
import fs from 'fs';
import {
  createMint,
  getAccount,
  getMint,
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
  TransactionInstruction,
  SYSVAR_RENT_PUBKEY,
  LAMPORTS_PER_SOL,
} from '@solana/web3.js';

//------------CONSTANTS------------
const connection = new Connection('http://127.0.0.1:8899', 'confirmed');

const ESCROW_PROGRAM_ID = new PublicKey('6rCwx3QNv8sBL2iiHwrDq7GvEj4wWZTEJY8VN1n6682R');
const LOGGER_PROGRAM_ID = new PublicKey('HFroz2wV8jgypuLEggSmZWTsxnnLNewjkfNX42UnFjyv');

const LOGGER_STATE_FILE = 'logger_state.json';
const USER1_FILE = 'wallet1.json';
const USER2_FILE = 'wallet2.json';

function getOrCreateKeypair(filePath: string, label: string): Keypair {
  if (fs.existsSync(filePath)) {
    console.log(`Loading existing wallet for ${label}`);
    const secretKey = Uint8Array.from(JSON.parse(fs.readFileSync(filePath, 'utf-8')));
    return Keypair.fromSecretKey(secretKey);
  } else {
    console.log(`Creating new wallet for ${label}`);
    const keypair = Keypair.generate();
    fs.writeFileSync(filePath, JSON.stringify(Array.from(keypair.secretKey)));
    return keypair;
  }
}

async function airdropIfNeeded(pubkey: PublicKey, label: string) {
  const balance = await connection.getBalance(pubkey);
  if (balance < 2 * LAMPORTS_PER_SOL) {
    console.log(`Airdropping 2 SOL to ${label}...`);
    const sig = await connection.requestAirdrop(pubkey, 2 * LAMPORTS_PER_SOL);
    await connection.confirmTransaction(sig);
  }
}

async function printLogsForTx(signature: string) {
  const txInfo = await connection.getTransaction(signature, { commitment: 'confirmed' });
  if (txInfo?.meta?.logMessages) {
    console.log('----- LOGS for Tx:', signature, '-----');
    for (const line of txInfo.meta.logMessages) {
      console.log(line);
    }
    console.log('----- END LOGS -----');
  } else {
    console.log('No transaction logs found for', signature);
  }
}

async function getLoggerSequence(statePubkey: PublicKey): Promise<number> {
  const accountInfo = await connection.getAccountInfo(statePubkey);
  if (!accountInfo) throw new Error(`LoggerState not found: ${statePubkey.toBase58()}`);
  if (accountInfo.data.length < 8) throw new Error('Invalid LoggerState');
  return Number(accountInfo.data.readBigUInt64LE(0));
}

async function getOrCreateLoggerStateAccount(payer: Keypair): Promise<Keypair> {
  if (fs.existsSync(LOGGER_STATE_FILE)) {
    console.log('Logger state found. Loading...');
    const secretKey = Uint8Array.from(JSON.parse(fs.readFileSync(LOGGER_STATE_FILE, 'utf-8')));
    return Keypair.fromSecretKey(secretKey);
  }
  console.log('🚀 Creating new logger state...');
  const kp = Keypair.generate();
  const space = 8;
  const lamports = await connection.getMinimumBalanceForRentExemption(space);
  const createIx = SystemProgram.createAccount({
    fromPubkey: payer.publicKey,
    newAccountPubkey: kp.publicKey,
    lamports,
    space,
    programId: LOGGER_PROGRAM_ID,
  });
  const tx = new Transaction().add(createIx);
  await sendAndConfirmTransaction(connection, tx, [payer, kp]);
  fs.writeFileSync(LOGGER_STATE_FILE, JSON.stringify(Array.from(kp.secretKey)));
  return kp;
}

async function getMessagePda(loggerProg: PublicKey, sequence: number): Promise<[PublicKey, number]> {
  const seqBuf = Buffer.alloc(8);
  seqBuf.writeBigUInt64LE(BigInt(sequence), 0);
  return PublicKey.findProgramAddress([Buffer.from('logger'), seqBuf], loggerProg);
}

async function main() {

  const user1 = getOrCreateKeypair(USER1_FILE, 'User1');
  const user2 = getOrCreateKeypair(USER2_FILE, 'User2');
  await airdropIfNeeded(user1.publicKey, 'User1');
  await airdropIfNeeded(user2.publicKey, 'User2');

  const loggerStateKP = await getOrCreateLoggerStateAccount(user1);
  console.log('LoggerState pubkey:', loggerStateKP.publicKey.toBase58());

  const mint = await createMint(connection, user1, user1.publicKey, null, 9);
  console.log('Mint:', mint.toBase58());

  const user1TokenAcc = await getOrCreateAssociatedTokenAccount(connection, user1, mint, user1.publicKey);
  const user2TokenAcc = await getOrCreateAssociatedTokenAccount(connection, user2, mint, user2.publicKey);

  await mintTo(connection, user1, mint, user1TokenAcc.address, user1, 100n);
  await mintTo(connection, user1, mint, user2TokenAcc.address, user1, 100n);
  const balance1 = await getAccount(connection, user1TokenAcc.address);
  const balance2 = await getAccount(connection, user2TokenAcc.address);
  console.log('User1 balance after withdraw:', Number(balance1.amount));
  console.log('User2 balance after withdraw:', Number(balance2.amount));
  const [escrowDataPda] = await PublicKey.findProgramAddress(
    [Buffer.from('escrow'), mint.toBuffer()],
    ESCROW_PROGRAM_ID
  );
  const [vaultPda] = await PublicKey.findProgramAddress(
    [Buffer.from('vault'), mint.toBuffer()],
    ESCROW_PROGRAM_ID
  );

  // ------------------ DEPOSIT ------------------
  const depositAmount = 50;
  const depositData = Buffer.alloc(1 + 8);
  depositData.writeUInt8(0, 0); 
  depositData.writeBigUInt64LE(BigInt(depositAmount), 1);

  const seqBefore = await getLoggerSequence(loggerStateKP.publicKey);
  const [messagePda] = await getMessagePda(LOGGER_PROGRAM_ID, seqBefore + 1);

  const depositIx = new TransactionInstruction({
    programId: ESCROW_PROGRAM_ID,
    keys: [
      { pubkey: user1.publicKey, isSigner: true, isWritable: true },
      { pubkey: user1TokenAcc.address, isSigner: false, isWritable: true },
      { pubkey: escrowDataPda, isSigner: false, isWritable: true },
      { pubkey: vaultPda, isSigner: false, isWritable: true },
      { pubkey: SystemProgram.programId, isSigner: false, isWritable: false },
      { pubkey: TOKEN_PROGRAM_ID, isSigner: false, isWritable: false },
      { pubkey: SYSVAR_RENT_PUBKEY, isSigner: false, isWritable: false },
      { pubkey: LOGGER_PROGRAM_ID, isSigner: false, isWritable: false },
      { pubkey: loggerStateKP.publicKey, isSigner: false, isWritable: true },
      { pubkey: messagePda, isSigner: false, isWritable: true },
      { pubkey: user1.publicKey, isSigner: true, isWritable: true },
      { pubkey: SystemProgram.programId, isSigner: false, isWritable: false },
      { pubkey: mint, isSigner: false, isWritable: false },
    ],
    data: depositData,
  });

  const depositTx = new Transaction().add(depositIx);
  const depositSig = await sendAndConfirmTransaction(connection, depositTx, [user1]);
  console.log('✅ Deposit successful. Signature:', depositSig);
  await printLogsForTx(depositSig);

  const balanceAfterDeposit = await getAccount(connection, user1TokenAcc.address);
  const vaultAfterDeposit = await getAccount(connection, vaultPda);
  console.log('User1 balance after deposit:', Number(balanceAfterDeposit.amount));
  console.log('Vault balance after deposit:', Number(vaultAfterDeposit.amount));

  // ------------------ WITHDRAW ------------------
  const withdrawAmount = 30;
  const withdrawData = Buffer.alloc(1 + 8);
  withdrawData.writeUInt8(1, 0); // 1 = Withdraw
  withdrawData.writeBigUInt64LE(BigInt(withdrawAmount), 1);

  const withdrawSeq = await getLoggerSequence(loggerStateKP.publicKey);
  const [withdrawMessagePda] = await getMessagePda(LOGGER_PROGRAM_ID, withdrawSeq + 1);

  const withdrawIx = new TransactionInstruction({
    programId: ESCROW_PROGRAM_ID,
    keys: [
      { pubkey: user1.publicKey, isSigner: true, isWritable: true },           
      { pubkey: user2TokenAcc.address, isSigner: false, isWritable: true },   
      { pubkey: escrowDataPda, isSigner: false, isWritable: true },            
      { pubkey: vaultPda, isSigner: false, isWritable: true },                 
      { pubkey: TOKEN_PROGRAM_ID, isSigner: false, isWritable: false },        
      { pubkey: LOGGER_PROGRAM_ID, isSigner: false, isWritable: false },       
      { pubkey: loggerStateKP.publicKey, isSigner: false, isWritable: true }, 
      { pubkey: vaultPda, isSigner: false, isWritable: false },                
      { pubkey: withdrawMessagePda, isSigner: false, isWritable: true },       
      { pubkey: user1.publicKey, isSigner: true, isWritable: true },           
      { pubkey: SystemProgram.programId, isSigner: false, isWritable: false },
    ],
    data: withdrawData,
  });

  const withdrawTx = new Transaction().add(withdrawIx);
  const withdrawSig = await sendAndConfirmTransaction(connection, withdrawTx, [user1]);
  console.log('✅ Withdraw successful. Signature:', withdrawSig);
  await printLogsForTx(withdrawSig);

  const balanceAfterWithdraw = await getAccount(connection, user1TokenAcc.address);
  const balance2AfterWithdraw = await getAccount(connection, user2TokenAcc.address);
  const vaultAfterWithdraw = await getAccount(connection, vaultPda);
  console.log('User1 balance after withdraw:', Number(balanceAfterWithdraw.amount));
  console.log('User2 balance after withdraw:', Number(balance2AfterWithdraw.amount));
  console.log('Vault balance after withdraw:', Number(vaultAfterWithdraw.amount));

}

main().catch(err => console.error('❌ Main failed:', err));