# Contributing

Contributions are welcome. **Security problems do not go here**: see [SECURITY.md](SECURITY.md).

## Before you start

- For anything larger than a fix, open an issue first and say what you want to change and why.
- By contributing you agree that your contribution is licensed under the licence in [LICENSE](LICENSE).
  There is no separate contributor agreement.
- Be decent: [CODE_OF_CONDUCT.md](CODE_OF_CONDUCT.md).

## Set up and run the checks

From a clone, Python 3.10 or later:

    python -m venv .venv
    # Windows PowerShell: .\.venv\Scripts\Activate.ps1   Linux/macOS: . .venv/bin/activate
    python -m pip install -e . pytest
    python -m pytest -q
    python spec/tools/make_vectors.py --check
    python scripts/check_install_lines.py
    ports/conformance/run.sh          # Python and every port whose toolchain is installed

## Rules a change must keep

1. No secrets, model keys, endpoints or personal data in the repository, and no council material that
   is not meant to be public (that lives in the private companion repository).
2. No runtime dependency: the package uses the standard library only, and no port has one.
3. A change to the engine, the gates, the schema checks, the exporters, the importer or the audit
   changes the vectors. Run `python spec/tools/make_vectors.py`, review the diff of `spec/conformance/`,
   copy the files to `src/sf_smartsavant/data/conformance/`, update `spec/SEMANTICS.md`, and update every
   port until `ports/conformance/run.sh` passes. The vectors are the contract.
4. Nothing may be invented on failure: an invalid or missing answer is recorded as such, never replaced
   with template text.
5. Say what was tested and what was not. Do not write that something works on a platform or at a scale
   it was not run on.
6. Install lines name only packages listed with `"registered": true` in `names.json`;
   `scripts/check_install_lines.py` enforces it in CI.
7. Add a line to [CHANGELOG.md](CHANGELOG.md) under "Unreleased".

## Pull requests

One topic per pull request. Describe what changes for a user, which tests cover it, and the commands
you ran with their results.
