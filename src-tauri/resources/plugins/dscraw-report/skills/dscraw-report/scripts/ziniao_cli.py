"""Run the installed Ziniao CLI without PowerShell or CMD script shims."""

import os
from pathlib import Path
import shutil
import subprocess
import sys


def find_cli():
    if os.name != "nt":
        executable = shutil.which("ziniao-cli")
        if executable:
            return executable
    else:
        executable = shutil.which("ziniao-cli.exe")
        if executable:
            return executable
        shim = shutil.which("ziniao-cli.cmd")
        roots = [Path(shim).parent] if shim else []
        if os.environ.get("APPDATA"):
            roots.append(Path(os.environ["APPDATA"]) / "npm")
        for root in roots:
            executable = root / "node_modules/@ziniao-open/cli/bin/ziniao-cli.exe"
            if executable.is_file():
                return str(executable)
    raise FileNotFoundError("找不到已安装的紫鸟 CLI 可执行文件；请检查安装位置和 PATH。")


def expand_script_file(args):
    args = list(args)
    if "--script-file" not in args:
        return args
    if args[:2] != ["page", "exec"]:
        raise ValueError("--script-file 仅用于 page exec。")
    if args.count("--script-file") != 1 or any(
        arg == "--script" or arg.startswith("--script=") for arg in args
    ):
        raise ValueError("只提供一个 --script-file，不要同时提供 --script。")
    index = args.index("--script-file")
    if index + 1 == len(args):
        raise ValueError("--script-file 后需要 UTF-8 JavaScript 文件路径。")
    script = Path(args[index + 1]).read_text(encoding="utf-8-sig")
    args[index:index + 2] = ["--script", script]
    return args


def main(args=None):
    try:
        args = expand_script_file(sys.argv[1:] if args is None else args)
        return subprocess.run([find_cli(), *args], shell=False).returncode
    except (OSError, ValueError) as error:
        print(f"ziniao_cli: {error}", file=sys.stderr)
        return 2


if __name__ == "__main__":
    raise SystemExit(main())
