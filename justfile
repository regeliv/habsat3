build-debug:
  cargo build --target aarch64-unknown-linux-gnu

  # Because we compile using nix toolchain, the program interpreter is some shared library in Nix store.
  # Our target does not have Nix store, therefore we override the interpreter path
  patchelf --set-interpreter /lib/ld-linux-aarch64.so.1 ./target/aarch64-unknown-linux-gnu/debug/air

  scp target/aarch64-unknown-linux-gnu/debug/air habsat@habsat.lan:~

build-release:
  cargo build --release --target aarch64-unknown-linux-gnu

  # Because we compile using nix toolchain, the program interpreter is some shared library in Nix store.
  # Our target does not have Nix store, therefore we override the interpreter path
  patchelf --set-interpreter /lib/ld-linux-aarch64.so.1 ./target/aarch64-unknown-linux-gnu/release/air

  scp target/aarch64-unknown-linux-gnu/release/air habsat@habsat.lan:~

mock-db-setup:
  mkdir --parents target/
  diesel setup --database-url target/mock.db
  diesel migration run --config-file apps/air/diesel.toml --database-url target/mock.db 

db-regen:
  diesel migration redo --config-file apps/air/diesel.toml --database-url target/mock.db 
