set -e

cargo build -p canister_a --target wasm32-unknown-unknown --release --lib
#cargo build -p canister_b --target wasm32-unknown-unknown --release

ic-cdk-optimizer target/wasm32-unknown-unknown/release/canister_a.wasm -o target/wasm32-unknown-unknown/release/canister_a-opt.wasm
#ic-cdk-optimizer target/wasm32-unknown-unknown/release/canister_b.wasm -o target/wasm32-unknown-unknown/release/canister_b-opt.wasm
