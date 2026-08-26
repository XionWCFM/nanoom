# ADR-0007: Streaming flat Action logs

- Status: Accepted
- Date: 2026-08-25

## Decision

Nanoom's four public Actions remain dependency-free Bash composite Actions. Each Action uses one shell step and prints `Inputs`, `Resolved values`, `Why`, `Command`, `Progress`, `Result`, `Action outputs`, and a compact one-line `Final JSON` in that order.

CLI JSON mode keeps exactly one JSON document on stdout. Child-process stdout and stderr stream to Nanoom's stderr while the process is running. Child `::group::` markers become plain headings and `::endgroup::` markers are omitted; other output and exit status are preserved.

JavaScript Action runtimes and toolkit dependencies are not allowed for this interface. GitHub's top-level workflow step and the composite shell wrapper remain collapsible because the runner owns them; Nanoom must not introduce another folded layer.

## Acceptance

- A delayed fake child proves its first line is observable before process exit, stdout remains parseable JSON, and group markers are flattened.
- The Action contract requires one Bash step, the standard section order, compact final JSON, structured failure JSON, and unchanged public outputs.
- Completion requires the normal repository gates and a released, pinned `nanoom-fixtures` run whose positive four-entry matrix executes install, task, and status with no Nanoom or child-tool log groups.
