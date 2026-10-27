import { getAddressDecoder, getStructDecoder, getU16Decoder } from '@solana/kit';

// Account data layout matching the native program's `Amm` struct:
// id (32 bytes), admin (32 bytes), fee (2 bytes u16)
export const ammDecoder = getStructDecoder([
    ['id', getAddressDecoder()],
    ['admin', getAddressDecoder()],
    ['fee', getU16Decoder()],
]);

// Account data layout matching the native program's `Pool` struct:
// amm (32 bytes), mint_a (32 bytes), mint_b (32 bytes)
export const poolDecoder = getStructDecoder([
    ['amm', getAddressDecoder()],
    ['mint_a', getAddressDecoder()],
    ['mint_b', getAddressDecoder()],
]);
