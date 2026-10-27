import {
    type Address,
    appendTransactionMessageInstructions,
    createTransactionMessage,
    generateKeyPairSigner,
    getAddressEncoder,
    getProgramDerivedAddress,
    type Instruction,
    type KeyPairSigner,
    pipe,
    setTransactionMessageFeePayerSigner,
    signTransactionMessageWithSigners,
} from '@solana/kit';
import { getCreateAccountInstruction } from '@solana-program/system';
import {
    findAssociatedTokenPda,
    getCreateAssociatedTokenIdempotentInstruction,
    getInitializeMint2Instruction,
    getMintSize,
    getMintToInstruction,
    TOKEN_PROGRAM_ADDRESS,
} from '@solana-program/token';
import { FailedTransactionMetadata, type LiteSVM } from 'litesvm';

export const AUTHORITY_SEED = new TextEncoder().encode('authority');
export const LIQUIDITY_SEED = new TextEncoder().encode('liquidity');

export async function findAssociatedTokenAddress(mint: Address, owner: Address): Promise<Address> {
    const [ata] = await findAssociatedTokenPda({
        mint,
        owner,
        tokenProgram: TOKEN_PROGRAM_ADDRESS,
    });
    return ata;
}

export async function findAmmPda(id: Address, programId: Address): Promise<[Address, number]> {
    const idBytes = getAddressEncoder().encode(id);
    return await getProgramDerivedAddress({
        programAddress: programId,
        seeds: [idBytes],
    });
}

export async function findPoolPda(
    amm: Address,
    mintA: Address,
    mintB: Address,
    programId: Address
): Promise<[Address, number]> {
    const addressEncoder = getAddressEncoder();
    return await getProgramDerivedAddress({
        programAddress: programId,
        seeds: [
            addressEncoder.encode(amm),
            addressEncoder.encode(mintA),
            addressEncoder.encode(mintB),
        ],
    });
}

export async function findPoolAuthorityPda(
    amm: Address,
    mintA: Address,
    mintB: Address,
    programId: Address
): Promise<[Address, number]> {
    const addressEncoder = getAddressEncoder();
    return await getProgramDerivedAddress({
        programAddress: programId,
        seeds: [
            addressEncoder.encode(amm),
            addressEncoder.encode(mintA),
            addressEncoder.encode(mintB),
            AUTHORITY_SEED,
        ],
    });
}

export async function findLiquidityMintPda(
    amm: Address,
    mintA: Address,
    mintB: Address,
    programId: Address
): Promise<[Address, number]> {
    const addressEncoder = getAddressEncoder();
    return await getProgramDerivedAddress({
        programAddress: programId,
        seeds: [
            addressEncoder.encode(amm),
            addressEncoder.encode(mintA),
            addressEncoder.encode(mintB),
            LIQUIDITY_SEED,
        ],
    });
}

export interface TestContext {
    programId: Address;
    admin: KeyPairSigner;
    depositor: KeyPairSigner;
    trader: KeyPairSigner;
    payer: KeyPairSigner;
    ammId: KeyPairSigner;
    ammPda: Address;
    mintA: KeyPairSigner;
    mintB: KeyPairSigner;
    poolPda: Address;
    poolAuthorityPda: Address;
    mintLiquidityPda: Address;
    poolAtaA: Address;
    poolAtaB: Address;
    depositorAtaA: Address;
    depositorAtaB: Address;
    depositorAtaLiquidity: Address;
    traderAtaA: Address;
    traderAtaB: Address;
}

export async function createTestContext(programId: Address): Promise<TestContext> {
    const admin = await generateKeyPairSigner();
    const depositor = await generateKeyPairSigner();
    const trader = await generateKeyPairSigner();
    const payer = await generateKeyPairSigner();
    const ammId = await generateKeyPairSigner();

    const mintA = await generateKeyPairSigner();
    const mintB = await generateKeyPairSigner();

    const [ammPda] = await findAmmPda(ammId.address, programId);
    const [poolPda] = await findPoolPda(ammPda, mintA.address, mintB.address, programId);
    const [poolAuthorityPda] = await findPoolAuthorityPda(ammPda, mintA.address, mintB.address, programId);
    const [mintLiquidityPda] = await findLiquidityMintPda(ammPda, mintA.address, mintB.address, programId);

    const poolAtaA = await findAssociatedTokenAddress(mintA.address, poolAuthorityPda);
    const poolAtaB = await findAssociatedTokenAddress(mintB.address, poolAuthorityPda);

    const depositorAtaA = await findAssociatedTokenAddress(mintA.address, depositor.address);
    const depositorAtaB = await findAssociatedTokenAddress(mintB.address, depositor.address);
    const depositorAtaLiquidity = await findAssociatedTokenAddress(mintLiquidityPda, depositor.address);

    const traderAtaA = await findAssociatedTokenAddress(mintA.address, trader.address);
    const traderAtaB = await findAssociatedTokenAddress(mintB.address, trader.address);

    return {
        programId,
        admin,
        depositor,
        trader,
        payer,
        ammId,
        ammPda,
        mintA,
        mintB,
        poolPda,
        poolAuthorityPda,
        mintLiquidityPda,
        poolAtaA,
        poolAtaB,
        depositorAtaA,
        depositorAtaB,
        depositorAtaLiquidity,
        traderAtaA,
        traderAtaB,
    };
}

export const setupMintAndAta = async ({
    svm,
    payer,
    owner,
    mintKeypair,
    initialAmount = 1_000_000n,
    decimals = 6,
}: {
    svm: LiteSVM;
    payer: KeyPairSigner;
    owner: KeyPairSigner;
    mintKeypair: KeyPairSigner;
    initialAmount?: bigint;
    decimals?: number;
}) => {
    const recipientAta = await findAssociatedTokenAddress(mintKeypair.address, owner.address);

    const createMintAccountIx = getCreateAccountInstruction({
        payer,
        newAccount: mintKeypair,
        lamports: 10_000_000n,
        space: BigInt(getMintSize()),
        programAddress: TOKEN_PROGRAM_ADDRESS,
    });

    const initMintIx = getInitializeMint2Instruction({
        mint: mintKeypair.address,
        decimals,
        mintAuthority: payer.address,
    });

    const createAtaIx = getCreateAssociatedTokenIdempotentInstruction({
        payer,
        owner: owner.address,
        mint: mintKeypair.address,
    });

    const mintToIx = getMintToInstruction({
        mint: mintKeypair.address,
        token: recipientAta,
        mintAuthority: payer,
        amount: initialAmount,
    });

    const msg = pipe(
        createTransactionMessage({ version: 0 }),
        m => setTransactionMessageFeePayerSigner(payer, m),
        m => svm.setTransactionMessageLifetimeUsingLatestBlockhash(m),
        m => appendTransactionMessageInstructions([createMintAccountIx, initMintIx, createAtaIx, mintToIx], m),
    );

    const signed = await signTransactionMessageWithSigners(msg);
    const result = svm.sendTransaction(signed);
    if (result instanceof FailedTransactionMetadata) {
        throw new Error(`setupMintAndAta failed: ${JSON.stringify(result.meta)}`);
    }

    return recipientAta;
};
