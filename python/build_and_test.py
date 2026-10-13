"""Build and test the Python bindings in an isolated local environment."""
import os
from pathlib import Path
import subprocess
import sys


def main():
    package = Path(__file__).resolve().parent
    environment = package.parent / ".github" / "local-output" / "python-bindings-env"
    subprocess.run([sys.executable, "-m", "venv", str(environment)], check=True)
    python = environment / ("Scripts/python.exe" if os.name == "nt" else "bin/python")
    subprocess.run([str(python), "-m", "pip", "install", "setuptools-rust", "pytest"], check=True)
    subprocess.run([str(python), "-m", "pip", "install", "-e", ".[tests]"], cwd=package, check=True)
    subprocess.run([str(python), "-m", "pytest", "-q"], cwd=package, check=True)


if __name__ == "__main__":
    main()
