import os
import subprocess
import sys
from pathlib import Path


def test_douyin_help_import_does_not_depend_on_working_directory(tmp_path):
    project_root = Path(__file__).resolve().parents[1]
    env = os.environ.copy()
    env["PYTHONPATH"] = str(project_root)

    result = subprocess.run(
        [sys.executable, "-c", "import media_platform.douyin.help"],
        cwd=tmp_path,
        env=env,
        capture_output=True,
        text=True,
        timeout=30,
    )

    assert result.returncode == 0, result.stderr
