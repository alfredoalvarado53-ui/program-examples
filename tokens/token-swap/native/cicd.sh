#!/bin/bash

# Quick build & deploy script for the token-swap-native program.
# Run with: bash cicd.sh   or   ./cicd.sh

cargo build-sbf --manifest-path=./program/Cargo.toml
solana program deploy ./program/target/deploy/token_swap_native_program.so
