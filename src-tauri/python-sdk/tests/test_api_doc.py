"""Keeps __init__.pyi, __init__.py and docs/API.md saying the same thing."""

from __future__ import annotations

import ast
import inspect
import re
import unittest
from pathlib import Path

import multialt

ROOT = Path(__file__).resolve().parents[1]
PYI = ROOT / "multialt" / "__init__.pyi"
API_MD = ROOT / "docs" / "API.md"
AGENTS_MD = ROOT / "AGENTS.md"


def _public_stub_names() -> tuple[list[str], list[tuple[str, str]]]:
    """(top-level names, (class, method) pairs) of the stub, ignoring private and dunder names."""
    tree = ast.parse(PYI.read_text(encoding="utf-8"))
    top: list[str] = []
    methods: list[tuple[str, str]] = []
    for node in tree.body:
        if isinstance(node, (ast.FunctionDef, ast.ClassDef)):
            if not node.name.startswith("_"):
                top.append(node.name)
            if isinstance(node, ast.ClassDef):
                for item in node.body:
                    if isinstance(item, ast.FunctionDef) and not item.name.startswith("_"):
                        methods.append((node.name, item.name))
        elif isinstance(node, ast.AnnAssign) and isinstance(node.target, ast.Name) and not node.target.id.startswith("_"):
            top.append(node.target.id)
        elif isinstance(node, ast.ImportFrom) and node.module == "_core.protocol":
            top.extend(alias.asname or alias.name for alias in node.names)
    return top, methods


def _params(func: ast.FunctionDef) -> list[tuple[str, str]]:
    """[(name, default source or "")] for every parameter except self/cls."""
    args = func.args
    positional = args.posonlyargs + args.args
    defaults = [None] * (len(positional) - len(args.defaults)) + list(args.defaults)
    out = [(a.arg, ast.unparse(d) if d is not None else "") for a, d in zip(positional, defaults)]
    if args.vararg:
        out.append(("*" + args.vararg.arg, ""))
    out += [(a.arg, ast.unparse(d) if d is not None else "") for a, d in zip(args.kwonlyargs, args.kw_defaults)]
    return [p for p in out if p[0] not in ("self", "cls")]


def _runtime_params(func: object) -> list[tuple[str, str]]:
    out = []
    for name, p in inspect.signature(func).parameters.items():  # type: ignore[arg-type]
        if name in ("self", "cls"):
            continue
        if p.kind is inspect.Parameter.VAR_POSITIONAL:
            out.append(("*" + name, ""))
        else:
            out.append((name, "" if p.default is inspect.Parameter.empty else repr(p.default)))
    return out


class ApiDocTests(unittest.TestCase):
    def test_every_public_name_of_the_stub_is_in_the_api_doc(self) -> None:
        doc = API_MD.read_text(encoding="utf-8")
        top, methods = _public_stub_names()
        missing = []
        for name in top:
            if not re.search(rf"\b{re.escape(name)}\b", doc):
                missing.append(name)
        for cls, method in methods:
            if not re.search(rf"\b{re.escape(method)}\(", doc):
                missing.append(f"{cls}.{method}")
        self.assertEqual(missing, [], "names missing from docs/API.md")

    def test_the_stub_matches_the_runtime_module(self) -> None:
        top, methods = _public_stub_names()
        for name in top:
            self.assertTrue(hasattr(multialt, name), f"stub has {name} but multialt does not")
        tree = ast.parse(PYI.read_text(encoding="utf-8"))
        for node in tree.body:
            if isinstance(node, ast.FunctionDef) and not node.name.startswith("_"):
                self.assertEqual(
                    _params(node), _runtime_params(getattr(multialt, node.name)), f"signature of {node.name}()"
                )
            if isinstance(node, ast.ClassDef) and hasattr(multialt, node.name):
                runtime_cls = getattr(multialt, node.name)
                for item in node.body:
                    if isinstance(item, ast.FunctionDef) and not item.name.startswith("_"):
                        self.assertEqual(
                            _params(item),
                            _runtime_params(getattr(runtime_cls, item.name)),
                            f"signature of {node.name}.{item.name}()",
                        )

    def test_every_public_runtime_name_is_in_the_stub(self) -> None:
        top, _methods = _public_stub_names()
        for name in multialt.__all__:
            self.assertIn(name, top, f"{name} is exported but missing from __init__.pyi")

    def test_every_error_code_is_explained_for_ai_tools(self) -> None:
        agents = AGENTS_MD.read_text(encoding="utf-8")
        doc = API_MD.read_text(encoding="utf-8")
        for code in (
            "not_foreground",
            "no_focus_lease",
            "untracked_window",
            "minimized",
            "forbidden",
            "focus_timeout",
            "unsupported_key",
            "bad_args",
            "capture_failed",
            "stopped",
        ):
            self.assertIn(f"`{code}`", agents, code)
            self.assertIn(f"`{code}`", doc, code)

    def test_the_developer_mode_message_matches_the_contract(self) -> None:
        agents = AGENTS_MD.read_text(encoding="utf-8")
        self.assertIn("Developer mode is off. Turn it on in MultiAlt > Scripts > Roblox Player automation > Developer.", agents)


if __name__ == "__main__":
    unittest.main()
