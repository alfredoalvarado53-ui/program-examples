import {
    AccountRole,
    type Address,
    getBooleanEncoder,
    getAddressEncoder,
    getStructEncoder,
    getU8Encoder,
    getU16Encoder,
    getU64Encoder,
    type TransactionSigner,
} from '@solana/kit';
import { SYSTEM_PROGRAM_ADDRESS } from '@solana-program/system';
import { ASSOCIATED_TOKEN_PROGRAM_ADDRESS, TOKEN_PROGRAM_ADDRESS } from '@solana-program/token';

export enum TokenSwapInstructionType {
    CreateAmm = 0,
    CreatePool = 1,
    DepositLiquidity = 2,
    SwapExactTokensForTokens = 3,
    WithdrawLiquidity = 4,
}

// 0: CreateAmm(CreateAmmArgs)
const createAmmEncoder = getStructEncoder([
    ['instruction', getU8Encoder()],
    ['id', getAddressEncoder()],
    ['fee', getU16Encoder()],
]);

// 1: CreatePool
const createPoolEncoder = getStructEncoder([
    ['instruction', getU8Encoder()],
]);

// 2: DepositLiquidity(DepositLiquidityArgs)
const depositLiquidityEncoder = getStructEncoder([
    ['instruction', getU8Encoder()],
    ['amount_a', getU64Encoder()],
    ['amount_b', getU64Encoder()],
]);

// 3: SwapExactTokensForTokens(SwapExactTokensForTokensArgs)
const swapExactTokensEncoder = getStructEncoder([
    ['instruction', getU8Encoder()],
    ['swap_a', getBooleanEncoder()],
    ['input_amount', getU64Encoder()],
    ['min_output_amount', getU64Encoder()],
]);

// 4: WithdrawLiquidity(WithdrawLiquidityArgs)
const withdrawLiquidityEncoder = getStructEncoder([
    ['instruction', getU8Encoder()],
    ['amount', getU64Encoder()],
]);

export function buildCreateAmm(props: {
    programId: Address;
    amm: Address;
    admin: Address;
    payer: TransactionSigner;
    id: Address;
    fee: number;
}) {
    const data = createAmmEncoder.encode({
        instruction: TokenSwapInstructionType.CreateAmm,
        id: props.id,
        fee: props.fee,
    });

    return {
        programAddress: props.programId,
        accounts: [
            { address: props.amm, role: AccountRole.WRITABLE },
            { address: props.admin, role: AccountRole.READONLY },
            { address: props.payer.address, role: AccountRole.WRITABLE_SIGNER },
            { address: SYSTEM_PROGRAM_ADDRESS, role: AccountRole.READONLY },
        ],
        data,
    };
}

export function buildCreatePool(props: {
    programId: Address;
    amm: Address;
    pool: Address;
    poolAuthority: Address;
    mintLiquidity: Address;
    mintA: Address;
    mintB: Address;
    poolAccountA: Address;
    poolAccountB: Address;
    payer: TransactionSigner;
}) {
    const data = createPoolEncoder.encode({
        instruction: TokenSwapInstructionType.CreatePool,
    });

    return {
        programAddress: props.programId,
        accounts: [
            { address: props.amm, role: AccountRole.READONLY },
            { address: props.pool, role: AccountRole.WRITABLE },
            { address: props.poolAuthority, role: AccountRole.READONLY },
            { address: props.mintLiquidity, role: AccountRole.WRITABLE },
            { address: props.mintA, role: AccountRole.READONLY },
            { address: props.mintB, role: AccountRole.READONLY },
            { address: props.poolAccountA, role: AccountRole.WRITABLE },
            { address: props.poolAccountB, role: AccountRole.WRITABLE },
            { address: props.payer.address, role: AccountRole.WRITABLE_SIGNER },
            { address: TOKEN_PROGRAM_ADDRESS, role: AccountRole.READONLY },
            { address: ASSOCIATED_TOKEN_PROGRAM_ADDRESS, role: AccountRole.READONLY },
            { address: SYSTEM_PROGRAM_ADDRESS, role: AccountRole.READONLY },
        ],
        data,
    };
}

export function buildDepositLiquidity(props: {
    programId: Address;
    pool: Address;
    poolAuthority: Address;
    depositor: TransactionSigner;
    mintLiquidity: Address;
    mintA: Address;
    mintB: Address;
    poolAccountA: Address;
    poolAccountB: Address;
    depositorAccountLiquidity: Address;
    depositorAccountA: Address;
    depositorAccountB: Address;
    payer: TransactionSigner;
    amountA: bigint;
    amountB: bigint;
}) {
    const data = depositLiquidityEncoder.encode({
        instruction: TokenSwapInstructionType.DepositLiquidity,
        amount_a: props.amountA,
        amount_b: props.amountB,
    });

    return {
        programAddress: props.programId,
        accounts: [
            { address: props.pool, role: AccountRole.READONLY },
            { address: props.poolAuthority, role: AccountRole.READONLY },
            { address: props.depositor.address, role: AccountRole.READONLY_SIGNER },
            { address: props.mintLiquidity, role: AccountRole.WRITABLE },
            { address: props.mintA, role: AccountRole.READONLY },
            { address: props.mintB, role: AccountRole.READONLY },
            { address: props.poolAccountA, role: AccountRole.WRITABLE },
            { address: props.poolAccountB, role: AccountRole.WRITABLE },
            { address: props.depositorAccountLiquidity, role: AccountRole.WRITABLE },
            { address: props.depositorAccountA, role: AccountRole.WRITABLE },
            { address: props.depositorAccountB, role: AccountRole.WRITABLE },
            { address: props.payer.address, role: AccountRole.WRITABLE_SIGNER },
            { address: TOKEN_PROGRAM_ADDRESS, role: AccountRole.READONLY },
            { address: ASSOCIATED_TOKEN_PROGRAM_ADDRESS, role: AccountRole.READONLY },
            { address: SYSTEM_PROGRAM_ADDRESS, role: AccountRole.READONLY },
        ],
        data,
    };
}

export function buildSwapExactTokensForTokens(props: {
    programId: Address;
    amm: Address;
    pool: Address;
    poolAuthority: Address;
    trader: TransactionSigner;
    mintA: Address;
    mintB: Address;
    poolAccountA: Address;
    poolAccountB: Address;
    traderAccountA: Address;
    traderAccountB: Address;
    payer: TransactionSigner;
    swapA: boolean;
    inputAmount: bigint;
    minOutputAmount: bigint;
}) {
    const data = swapExactTokensEncoder.encode({
        instruction: TokenSwapInstructionType.SwapExactTokensForTokens,
        swap_a: props.swapA,
        input_amount: props.inputAmount,
        min_output_amount: props.minOutputAmount,
    });

    return {
        programAddress: props.programId,
        accounts: [
            { address: props.amm, role: AccountRole.READONLY },
            { address: props.pool, role: AccountRole.READONLY },
            { address: props.poolAuthority, role: AccountRole.READONLY },
            { address: props.trader.address, role: AccountRole.READONLY_SIGNER },
            { address: props.mintA, role: AccountRole.READONLY },
            { address: props.mintB, role: AccountRole.READONLY },
            { address: props.poolAccountA, role: AccountRole.WRITABLE },
            { address: props.poolAccountB, role: AccountRole.WRITABLE },
            { address: props.traderAccountA, role: AccountRole.WRITABLE },
            { address: props.traderAccountB, role: AccountRole.WRITABLE },
            { address: props.payer.address, role: AccountRole.WRITABLE_SIGNER },
            { address: TOKEN_PROGRAM_ADDRESS, role: AccountRole.READONLY },
            { address: ASSOCIATED_TOKEN_PROGRAM_ADDRESS, role: AccountRole.READONLY },
            { address: SYSTEM_PROGRAM_ADDRESS, role: AccountRole.READONLY },
        ],
        data,
    };
}

export function buildWithdrawLiquidity(props: {
    programId: Address;
    amm: Address;
    pool: Address;
    poolAuthority: Address;
    depositor: TransactionSigner;
    mintLiquidity: Address;
    mintA: Address;
    mintB: Address;
    poolAccountA: Address;
    poolAccountB: Address;
    depositorAccountLiquidity: Address;
    depositorAccountA: Address;
    depositorAccountB: Address;
    amount: bigint;
}) {
    const data = withdrawLiquidityEncoder.encode({
        instruction: TokenSwapInstructionType.WithdrawLiquidity,
        amount: props.amount,
    });

    return {
        programAddress: props.programId,
        accounts: [
            { address: props.amm, role: AccountRole.READONLY },
            { address: props.pool, role: AccountRole.READONLY },
            { address: props.poolAuthority, role: AccountRole.READONLY },
            { address: props.depositor.address, role: AccountRole.READONLY_SIGNER },
            { address: props.mintLiquidity, role: AccountRole.WRITABLE },
            { address: props.mintA, role: AccountRole.READONLY },
            { address: props.mintB, role: AccountRole.READONLY },
            { address: props.poolAccountA, role: AccountRole.WRITABLE },
            { address: props.poolAccountB, role: AccountRole.WRITABLE },
            { address: props.depositorAccountLiquidity, role: AccountRole.WRITABLE },
            { address: props.depositorAccountA, role: AccountRole.WRITABLE },
            { address: props.depositorAccountB, role: AccountRole.WRITABLE },
            { address: TOKEN_PROGRAM_ADDRESS, role: AccountRole.READONLY },
        ],
        data,
    };
}
