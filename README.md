to set number of particles, go to `src/main.rs` and set `NUM_PARTICLES` to whatever you want the number of particles to be

to run in release mode, do

`cargo run --release`

for web version in release mode, do

`rustup target add wasm32-unknown-unknown`

`cargo run --release --target wasm32-unknown-unknown`
