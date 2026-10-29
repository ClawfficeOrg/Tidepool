"""The package builds and imports from a wheel with no optional extra."""

from __future__ import annotations

import pathlib
import shutil
import subprocess
import sys
import tomllib
import zipfile

REPOSITORY = pathlib.Path(__file__).resolve().parents[3]


def test_pyproject_declares_no_runtime_or_optional_dependencies():
    with (REPOSITORY / "pyproject.toml").open("rb") as handle:
        metadata = tomllib.load(handle)
    project = metadata["project"]
    assert project["name"] == "tidepool-client"
    assert project["requires-python"] == ">=3.9"
    assert project["dependencies"] == []
    assert "optional-dependencies" not in project


def test_wheel_contains_the_package_and_imports_without_installing(tmp_path):
    source = tmp_path / "source"
    source.mkdir()
    shutil.copy(REPOSITORY / "pyproject.toml", source / "pyproject.toml")
    shutil.copytree(
        REPOSITORY / "sdk/python",
        source / "sdk/python",
        ignore=shutil.ignore_patterns("__pycache__", "*.pyc"),
    )
    dist = tmp_path / "dist"
    result = subprocess.run(
        [
            sys.executable,
            "-m",
            "pip",
            "wheel",
            "--no-build-isolation",
            "--no-deps",
            "--wheel-dir",
            str(dist),
            str(source),
        ],
        capture_output=True,
        text=True,
        timeout=180,
    )
    assert result.returncode == 0, result.stderr
    wheels = sorted(dist.glob("tidepool_client-*.whl"))
    assert len(wheels) == 1, result.stdout
    wheel = wheels[0]
    with zipfile.ZipFile(wheel) as archive:
        names = set(archive.namelist())
        assert "tidepool_client/__init__.py" in names
        assert "tidepool_client/_generated/models.py" in names
        assert "tidepool_client/_generated/routes.py" in names
        assert "tidepool_client/_generated/resources.py" in names
        assert "tidepool_client/py.typed" in names
        metadata_name = next(name for name in names if name.endswith(".dist-info/METADATA"))
        metadata = archive.read(metadata_name).decode("utf-8")
    requires = [line for line in metadata.splitlines() if line.startswith("Requires-Dist:")]
    assert requires == []
    assert "Provides-Extra:" not in metadata

    # Import straight from the wheel through zipimport: no installation, no
    # network, and it proves the wheel is self-contained.
    probe = (
        "import sys; "
        f"sys.path.insert(0, {str(wheel)!r}); "
        "import tidepool_client; "
        "from tidepool_client import SyncClient, AsyncClient; "
        "print(len(tidepool_client.ROUTES), tidepool_client.__version__)"
    )
    imported = subprocess.run(
        [sys.executable, "-c", probe], capture_output=True, text=True, timeout=60
    )
    assert imported.returncode == 0, imported.stderr
    assert imported.stdout.strip() == "85 0.1.0"
