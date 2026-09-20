# Distribution verification

Verified locally on 2026-09-20 using Python 3.12 on macOS ARM64.

| Check | Result |
|---|---|
| Fresh virtual environment, `pip install -e '.[dev]'` | Passed |
| Installed `jpp demo` | Passed; target 731 in 10 questions; expression in 3 trials, 9 inputs checked |
| `python -m pytest -q` | 25 passed; two expected budget-exhaustion warnings |
| Wheel build | Passed |
| Publication file scan | No credential-shaped strings or private absolute paths detected |

The credential scan is a scoped check, not a general security audit. Test observations and calibration entries are synthetic. Live model accuracy and service compatibility were not measured in this release preparation.

CI runs the offline example and tests on Linux with Python 3.12 and 3.13. The Actions page is the source of truth for remote run status.
