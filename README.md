# The Rust Programming Language (Coverage for RustSmith Tools Paper)

## Instructions for (re)computing coverage of Rust test suite (in Docker container)
- This repository has been set up with the correct `config.toml` and coverage instrumentation in `builder.rs`. The Docker container will also have already ran the build script.
- Clear existing coverage data:
   - `rm -r coverage/rustsmith/_html/* && rm -r coverage/oots/_html/*`
- For OOTS:
   - Generate `.profraw` files, for all tests in `mir-opt`, containing coverage information
      - `LLVM_PROFILE_FILE="coverage/oots/%p-%m.profraw" ./x.py test src/test/mir-opt --force-rerun`
   - Generate coverage `html` files
      - `grcov coverage/oots/*.profraw -s compiler -b /app/code-coverage/build/x86_64-unknown-linux-gnu --llvm-path /app/code-coverage/build/x86_64-unknown-linux-gnu/llvm/bin -t html -o coverage/oots/_html`
- For RustSmith:
   - Generate 1000 programs:
      - `/app/rustsmith/bin/rustsmith -n 1000 --directory coverage/rustsmith/files`
   - Compile generated programs to generate `.profraw` files:
      - `LLVM_PROFILE_FILE="coverage/rustsmith/%p-%m.profraw" python compile_rustsmith_files.py`
   - Generate coverage `html` files
      - `grcov coverage/rustsmith/*.profraw -s compiler -b /app/code-coverage/build/x86_64-unknown-linux-gnu --llvm-path /app/code-coverage/build/x86_64-unknown-linux-gnu/llvm/bin -t html -o coverage/rustsmith/_html`
- Viewing the results
   - Start up a simple Python server for the HTML files on port 8080:
      - `python -m http.server 8080 --bind 0.0.0.0 --directory /app/code-coverage/coverage`
   - The coverage data can then be viewed from the host machine (outside the Docker container) at `localhost:8080`, within the respective `_html` folders.