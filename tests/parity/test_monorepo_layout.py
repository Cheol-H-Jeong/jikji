from __future__ import annotations

import tomllib
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]


def test_root_declares_python_workspace_member() -> None:
    pyproject = tomllib.loads((ROOT / "pyproject.toml").read_text(encoding="utf-8"))

    assert pyproject["tool"]["uv"]["workspace"]["members"] == ["python/jikji"]


def test_python_package_owns_jikji_src_layout() -> None:
    package_pyproject = tomllib.loads(
        (ROOT / "python" / "jikji" / "pyproject.toml").read_text(encoding="utf-8")
    )

    assert package_pyproject["project"]["name"] == "jikji"
    assert package_pyproject["project"]["scripts"]["jikji"] == "jikji.__main__:main"
    assert package_pyproject["tool"]["setuptools"]["packages"]["find"]["where"] == ["src"]
    assert (ROOT / "python" / "jikji" / "src" / "jikji" / "__main__.py").is_file()
    assert not (ROOT / "src" / "jikji" / "__main__.py").exists()
