# Security policy

SmartSavant 0.1.0 is experimental software. Before release it had one code review by a separate AI
reviewer that did not write the code. It has had no independent human audit.
[README "Status"](README.md#status) says what is tested and what is not.

## Supported versions

| Version | Security fixes |
|---|---|
| 0.1.x | yes |

## Reporting a vulnerability

Report privately through GitHub: open <https://github.com/SmartTasksOrg/sf-smartsavant>, go to the
**Security** tab and choose **Report a vulnerability** (GitHub private vulnerability reporting). Only
the maintainers can read the report.

Do not open a public issue, pull request or discussion for a vulnerability, and do not put a working
exploit in one.

If the Security tab shows no "Report a vulnerability" button, private reporting is switched off. Open a
public issue that says only "I have a security report; please enable private vulnerability reporting",
with no details, and wait for a maintainer.

## What to include

- The version (`sf-smartsavant --version`) or the port, and your platform.
- The smallest input that shows the problem, and what you expected.

Remove real secrets, tokens and personal data from what you send.

## Packages that pretend to be SmartSavant

The only package names the maintainers publish are listed with `"registered": true` in
[names.json](names.json). Official releases are built by `.github/workflows/release.yml` in
`SmartTasksOrg/sf-smartsavant` and published with trusted publishing, so each file carries a provenance
attestation that names this repository and that workflow.

A package on any registry that uses this project's name but is not listed there, or a release file
without that provenance, is not ours. Report it through the private form above, and to the registry
(PyPI: "Report project as malware" on the project page).

## What to expect

- An answer in the private report. No response time is promised; this is a small project without a
  security team on call.
- A confirmed issue is fixed in a new release, announced in `CHANGELOG.md` and in a GitHub security
  advisory, with credit to the reporter unless they ask not to be named.
- There is no bug bounty.

## Scope

In scope: the code in this repository and its build and release files. Especially:
- an adapter answer that violates the contract but is used anyway (in a view, a merge or an export);
- a phase reported as decided (`full`) without a decision, or fallback text that passes `synthesis_ok`;
- trainset rows from synthetic runs, failed or thin merges, or flagged views;
- model text that injects markup into `spec.md`, the Mermaid tree or a terminal;
- an adapter that can change the run's records through the arguments it receives;
- reading a zip or jsonl in `audit` that writes files, follows paths or exhausts memory beyond the documented limits;
- two engines (Python or any port in `ports/`) giving different results for the same input.

Out of scope (docs/threat-model.md):
- prompt injection through the idea or the sources into your adapter's model;
- code loaded with `--adapter module:Class`;
- decisions that are wrong but well-formed;
- the keyword compliance stub missing content (it is a stub).
