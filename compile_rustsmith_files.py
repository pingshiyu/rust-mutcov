import subprocess
from pathlib import Path
from multiprocessing import Pool

RUST_PATH = "/app/code-coverage/build/x86_64-unknown-linux-gnu/stage1/bin/rustc"
ROOT = Path("./coverage/rustsmith/files")
TIMEOUT_SECONDS = 60
JOBS=8

def compile_file(file: Path) -> bool:
    compile_cmd = [RUST_PATH, "-Zmir-opt-level=4", "-Copt-level=1", "-o", "out.o", file]
    try:
        subprocess.run(compile_cmd, timeout=TIMEOUT_SECONDS)
        print(f"compiled {file.name}")
    except subprocess.TimeoutExpired as e:
        print(f"compiling {file} timed out.")
        return False
    return True

if __name__ == '__main__':
    all_rust_files = list(ROOT.rglob("*.rs"))
    with Pool(processes=JOBS) as p:
        async_results = [p.apply_async(compile_file, [file]) for file in all_rust_files]

        successes = 0
        for result in async_results:
            compiled = result.get()
            if compiled: successes += 1

        print(f"total compiled: {successes}/{len(all_rust_files)}")