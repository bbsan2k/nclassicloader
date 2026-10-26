#!/usr/bin/env python3
"""Check publishable files (or the Git index) for accidental private payloads.

This is a narrow content-policy check, not a complete secret scanner or legal
review. Ignored local input images and build directories are never traversed.
"""
import argparse
from pathlib import Path, PurePosixPath
import re
import subprocess
from urllib.parse import unquote, urlsplit

ROOT = Path(__file__).resolve().parents[1]
MAX_BYTES = 5 * 1024 * 1024
SOURCE_FONTS = {
    "rust/assets/fonts/Barlow-Regular.ttf",
    "rust/assets/fonts/Barlow-SemiBold.ttf",
}
PRIVATE_SUFFIXES = {
    ".img", ".hsqs", ".squashfs", ".nes", ".fds", ".qd", ".sfc", ".smc",
    ".sfrom", ".rom", ".bin", ".iso", ".state", ".sram", ".sav", ".srm",
    ".suspend", ".png", ".jpg", ".jpeg", ".webp", ".gif", ".bmp", ".wav",
    ".mp3", ".ogg", ".flac", ".lng", ".scn", ".spriteanim", ".fnt", ".font",
    ".ttf", ".otf", ".zip", ".7z", ".xz", ".tar", ".gz", ".tgz", ".app",
    ".o", ".obj", ".a", ".lib", ".so", ".dylib", ".dll", ".exe", ".rlib",
    ".rmeta", ".p12", ".pfx", ".pem", ".key", ".mobileprovision", ".pyc", ".ipa", ".xcuserstate",
}
PRIVATE_ROOTS = {".build", "dist", "analysis", "NES-Mini-Kernels", "SNES-Mini-Kernels"}
SECRETS = (
    re.compile(r"-----BEGIN (?:RSA |EC |OPENSSH |DSA |ENCRYPTED )?PRIVATE KEY-----"),
    re.compile(r"\bgh[pousr]_[A-Za-z0-9]{36,}\b"),
    re.compile(r"\bgithub_pat_[A-Za-z0-9_]{70,}\b"),
    re.compile(r"\bAKIA[A-Z0-9]{16}\b"),
)
LINK = re.compile(r"\[[^\]\n]*\]\(([^)\n]+)\)")


def git(root, *arguments):
    return subprocess.check_output(["git", "-C", str(root), *arguments])


def path_problems(name):
    path = PurePosixPath(name)
    parts = path.parts
    problems = []
    if not parts or path.is_absolute() or ".." in parts:
        return ["unsafe repository path"]
    if (parts[0] in PRIVATE_ROOTS or parts[0].startswith("import-test-")
            or parts[:2] in (("third_party", "SDL"), ("third_party", "ares"), ("third_party", "zlib"))
            or any(part in {"target", "__pycache__", ".git"}
                   or part.lower().endswith((".app", ".dsym", ".xcarchive", ".xcresult", ".xcuserdata")) for part in parts)):
        problems.append("build output, private input or upstream checkout")
    if path.suffix.lower() in PRIVATE_SUFFIXES and name not in SOURCE_FONTS:
        problems.append("console content, binary artifact or credential container")
    if (path.name == ".DS_Store" or path.name == ".env"
            or path.name.startswith(".env.") and path.name != ".env.example"):
        problems.append("local environment or workstation file")
    return problems


def content_problems(name, data, check_workflow_pins=True):
    problems = path_problems(name)
    if len(data) > MAX_BYTES:
        problems.append("file exceeds the 5 MiB source-content limit")
    if name in SOURCE_FONTS:
        if not data.startswith(b"\x00\x01\x00\x00"):
            problems.append("expected the explicitly licensed TrueType source font")
        return problems
    try:
        text = data.decode("utf-8")
    except UnicodeDecodeError:
        return problems + ["unexpected binary or non-UTF-8 source file"]
    if "\x00" in text:
        problems.append("unexpected NUL bytes in source")
    for number, line in enumerate(text.splitlines(), 1):
        if any(pattern.search(line) for pattern in SECRETS):
            problems.append(f"possible credential at line {number} (value omitted)")
        if line.startswith(("<<<<<<< ", ">>>>>>> ")):
            problems.append(f"unresolved merge marker at line {number}")
        if check_workflow_pins and name.startswith(".github/workflows/") and line.strip().startswith(("uses:", "- uses:")):
            values = line.split("uses:", 1)[1].strip().split()
            action = values[0] if values else ""
            if not action.startswith("./") and not re.fullmatch(r"[A-Za-z0-9_./-]+@[0-9a-f]{40}", action):
                problems.append(f"external workflow action is not pinned to a full commit at line {number}")
    return problems


def link_problems(name, data, candidates):
    if not name.endswith(".md"):
        return []
    problems = []
    fenced = False
    for number, line in enumerate(data.decode("utf-8", errors="replace").splitlines(), 1):
        if line.lstrip().startswith(("```", "~~~")):
            fenced = not fenced
            continue
        if fenced:
            continue
        for match in LINK.finditer(line):
            target = match[1].strip()
            if target.startswith("<") and ">" in target:
                target = target[1:target.index(">")]
            else:
                target = target.split(' "', 1)[0]
            parsed = urlsplit(target)
            if parsed.scheme or parsed.netloc or not parsed.path:
                continue
            relative = PurePosixPath(name).parent / unquote(parsed.path)
            normalized = []
            for part in relative.parts:
                if part == "..":
                    if normalized:
                        normalized.pop()
                    else:
                        normalized.append("..")
                elif part != ".":
                    normalized.append(part)
            resolved = "/".join(normalized).rstrip("/")
            exists = resolved in candidates or any(p.startswith(resolved + "/") for p in candidates)
            if not exists:
                problems.append(f"relative link does not resolve to published content at line {number} (target omitted)")
    return problems


def inspect_files(root, staged=False):
    if staged:
        names = git(root, "ls-files", "-z").decode().split("\0")
    else:
        names = git(root, "ls-files", "-z", "--cached", "--others", "--exclude-standard").decode().split("\0")
    candidates = {name for name in names if name}
    problems = []
    for name in sorted(candidates):
        path = root / name
        if staged:
            entries = [line.split(None, 3) for line in git(root, "ls-files", "--stage", "--", name).decode().splitlines()]
            if len(entries) != 1 or entries[0][0] not in {"100644", "100755"} or entries[0][2] != "0":
                problems.append((name, "symlink, submodule or unresolved index entry"))
                continue
            data = git(root, "show", ":" + name)
        else:
            if path.is_symlink() or not path.is_file():
                problems.append((name, "missing file, symlink or submodule; review the index"))
                continue
            data = path.read_bytes()
        for issue in content_problems(name, data) + link_problems(name, data, candidates):
            problems.append((name, issue))
    return len(candidates), problems


def inspect_history(root):
    """Inspect every reachable blob without rewriting history or printing secrets."""
    if git(root, "rev-parse", "--is-shallow-repository").strip() == b"true":
        return 0, [("Git history", "shallow checkout; fetch full history before requesting a history audit")]
    objects = git(root, "rev-list", "--objects", "--all").decode().splitlines()
    problems = []
    count = 0
    for entry in objects:
        oid, separator, name = entry.partition(" ")
        if not separator or git(root, "cat-file", "-t", oid).strip() != b"blob":
            continue
        count += 1
        size = int(git(root, "cat-file", "-s", oid))
        issues = path_problems(name)
        if size > MAX_BYTES:
            issues.append("historical blob exceeds the 5 MiB source-content limit")
        else:
            issues = content_problems(name, git(root, "cat-file", "blob", oid), check_workflow_pins=False)
        for issue in issues:
            problems.append((f"{name} ({oid[:12]})", issue))
    return count, problems


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--staged", action="store_true", help="Read the actual Git index instead of working-tree candidates")
    parser.add_argument("--history", action="store_true", help="Also check blobs reachable from all local Git refs")
    args = parser.parse_args()
    count, problems = inspect_files(ROOT, args.staged)
    history_count = 0
    if args.history:
        history_count, historical = inspect_history(ROOT)
        problems.extend(historical)
    for name, issue in problems:
        for pattern in SECRETS:
            name = pattern.sub("[credential omitted]", name)
        print(f"ERROR: {name}: {issue}")
    if problems:
        raise SystemExit(f"Repository check failed: {len(problems)} finding(s). No files or history were changed.")
    print(f"Repository check passed: {count} publishable files; {history_count} historical blobs checked.")
    print("Review staged changes manually; this is not a complete secret scan or legal clearance.")


if __name__ == "__main__":
    main()
