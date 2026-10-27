# cnft-burn

This repository contains the cnft-burn program, a Solana Anchor program that allows you to burn compressed NFTs (cNFTs) in your collection. The program interacts with the Metaplex Bubblegum program through CPI to burn cNFTs.

## Components

- programs: Contains the anchor program
- programs/cnft-burn/tests: Rust LiteSVM tests that create a tree, mint a cNFT and burn it against the mainnet Bubblegum, account compression and noop programs

## Deployment

The program is deployed on devnet at `FbeHkUEevbhKmdk5FE5orcTaJkCYn5drwZoZXaxQXXNn`. You can deploy it yourself by changing the respective values in lib.rs and Anchor.toml.

## How to run

1. run `anchor build` at the root of the project i.e cnft-burn in this case.
2. run `anchor test` to run the tests. The first run dumps the required mainnet programs into `tests/fixtures` with `solana program dump`.

## Acknowledgements

This Example program would not have been possible without the work of:

- [Metaplex](https://github.com/metaplex-foundation/) for providing the Bubblegum program with ix builders.
- [@nickfrosty](https://twitter.com/nickfrosty) for providing the sample code for fetching and creating cNFTs.
