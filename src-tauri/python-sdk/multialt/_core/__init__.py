"""MultiAlt core: the transport and the thin wrappers around the app's primitives.

App-owned. MultiAlt rewrites this folder every time it starts, so do not edit
it. The things you can edit live in ``multialt/defaults/`` (developer mode).
"""

from __future__ import annotations

from pathlib import Path

# <workspace>/multialt/_core/__init__.py -> parents[2] is the workspace folder
# (the one that holds scripts/, lib/ and assets/).
WORKSPACE_DIR: Path = Path(__file__).resolve().parents[2]
SCRIPTS_DIR: Path = WORKSPACE_DIR / "scripts"
ASSETS_DIR: Path = WORKSPACE_DIR / "assets"
PIPE_FILE: Path = Path(__file__).resolve().parent / "pipe.txt"
