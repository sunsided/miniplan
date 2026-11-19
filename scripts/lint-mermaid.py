#!/usr/bin/env python3
"""Render every mermaid diagram in the repository with mermaid-cli and report broken ones.

    python3 lint-mermaid.py [paths...]

Without paths it scans the git-tracked `*.md` and `*.rs` files. Diagrams are ```mermaid fenced
blocks in Markdown and in Rust doc comments (`//!`, `///`). Each one is rendered with `mmdc`; a
diagram that fails to render is reported as `file:line` and makes the script exit non-zero.

Environment:
    MMDC             command to run mermaid-cli (default: `mmdc`), for example
                     `npx --yes @mermaid-js/mermaid-cli@12.2.0`
    MMDC_NO_SANDBOX  set to 1 on CI runners where Chromium cannot use its sandbox
"""

import json
import os
import re
import shlex
import shutil
import subprocess
import sys
import tempfile
from pathlib import Path

DOC_PREFIX = re.compile(r"^\s*//[/!]\s?")
FENCE_OPEN = re.compile(r"^\s*```\s*mermaid\s*$")
FENCE_CLOSE = re.compile(r"^\s*```\s*$")


def tracked_files():
    """Return the git-tracked Markdown and Rust files."""
    out = subprocess.check_output(["git", "ls-files", "*.md", "*.rs"], text=True)
    return [Path(line) for line in out.splitlines() if line and Path(line).is_file()]


def doc_lines(path):
    """Yield (line number, text) pairs, with Rust doc-comment prefixes stripped."""
    is_rust = path.suffix == ".rs"

    for number, line in enumerate(path.read_text(encoding="utf-8").splitlines(), start=1):
        if not is_rust:
            yield number, line
        elif DOC_PREFIX.match(line):
            yield number, DOC_PREFIX.sub("", line, count=1)
        else:
            # A non-doc line ends any open block in Rust sources.
            yield number, None


def diagrams(path):
    """Yield (start line, source) for each mermaid block in a file."""
    start, body = None, []

    for number, text in doc_lines(path):
        if start is None:
            if text is not None and FENCE_OPEN.match(text):
                start, body = number, []

            continue

        if text is None or FENCE_CLOSE.match(text):
            yield start, "\n".join(body) + "\n"
            start = None
            continue

        body.append(text)


def puppeteer_args(workdir):
    """Return extra mmdc arguments that disable the Chromium sandbox when requested."""
    if os.environ.get("MMDC_NO_SANDBOX") != "1":
        return []

    config = Path(workdir) / "puppeteer.json"
    config.write_text(json.dumps({"args": ["--no-sandbox"]}), encoding="utf-8")

    return ["-p", str(config)]


def render(mmdc, extra, source, workdir, index):
    """Render one diagram; return the error output, or None on success."""
    src = Path(workdir) / f"d{index}.mmd"
    src.write_text(source, encoding="utf-8")
    cmd = [*mmdc, "-q", "-i", str(src), "-o", str(src.with_suffix(".svg")), *extra]
    result = subprocess.run(cmd, capture_output=True, text=True, check=False)

    return None if result.returncode == 0 else (result.stderr or result.stdout).strip()


def main():
    """Entry point."""
    files = [Path(p) for p in sys.argv[1:]] or tracked_files()
    found = [(path, line, src) for path in files for line, src in diagrams(path)]

    if not found:
        print("no mermaid diagrams found")
        return

    mmdc = shlex.split(os.environ.get("MMDC", "mmdc"))

    if shutil.which(mmdc[0]) is None:
        sys.exit(
            f"{len(found)} diagram(s) found but {mmdc[0]!r} is missing: "
            "npm install -g @mermaid-js/mermaid-cli, or set MMDC"
        )

    failures = 0

    with tempfile.TemporaryDirectory() as workdir:
        extra = puppeteer_args(workdir)

        for index, (path, line, source) in enumerate(found):
            error = render(mmdc, extra, source, workdir, index)

            if error is not None:
                failures += 1
                print(f"{path}:{line}: mermaid diagram failed to render\n{error}\n")

    print(f"{len(found)} diagram(s) checked, {failures} broken")
    sys.exit(1 if failures else 0)


if __name__ == "__main__":
    main()
