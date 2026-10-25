import { describe, expect, it } from "vitest";
import { makeWorkspaceTree } from "../../test-utils/pythonWorkspace";
import {
  canRenameOrDelete,
  findNode,
  freeCopyName,
  isValidSegment,
  targetDirFor,
  treeSignature,
  withDefaultExtension,
} from "./workspaceTree";

const tree = makeWorkspaceTree();

describe("workspace tree helpers", () => {
  it("finds nodes by path", () => {
    expect(findNode(tree, "scripts/tools/util.py")?.name).toBe("util.py");
    expect(findNode(tree, "scripts/missing.py")).toBeNull();
  });

  it("puts new entries in the selected writable folder, or next to the selected file, or in scripts", () => {
    expect(targetDirFor(tree, "lib")).toBe("lib");
    expect(targetDirFor(tree, "lib/helpers.py")).toBe("lib");
    expect(targetDirFor(tree, "scripts/tools/util.py")).toBe("scripts/tools");
    expect(targetDirFor(tree, "examples")).toBe("scripts");
    expect(targetDirFor(tree, "multialt/defaults/runner.py")).toBe("scripts");
    expect(targetDirFor(tree, null)).toBe("scripts");
  });

  it("only renames and deletes what the user owns", () => {
    expect(canRenameOrDelete(findNode(tree, "scripts/farm.py")!)).toBe(true);
    expect(canRenameOrDelete(findNode(tree, "scripts")!)).toBe(false);
    expect(canRenameOrDelete(findNode(tree, "multialt/defaults/runner.py")!)).toBe(false);
    expect(canRenameOrDelete(findNode(tree, "examples/walk_and_jump.py")!)).toBe(false);
  });

  it("adds .py in scripts and lib only", () => {
    expect(withDefaultExtension("scripts", "farm")).toBe("farm.py");
    expect(withDefaultExtension("lib/sub", "farm")).toBe("farm.py");
    expect(withDefaultExtension("scripts", "notes.txt")).toBe("notes.txt");
    expect(withDefaultExtension("assets", "button")).toBe("button");
  });

  it("checks one path segment like the backend", () => {
    expect(isValidSegment("my farm-2.py")).toBe(true);
    for (const bad of ["", ".hidden", "../x", "a/b", "a\\b", "c:x", "x".repeat(65)]) expect(isValidSegment(bad)).toBe(false);
  });

  it("names a copy that does not collide", () => {
    expect(freeCopyName(tree, "examples/new.py")).toBe("new.py");
    expect(freeCopyName(tree, "examples/farm.py")).toBe("farm_copy.py");
  });

  it("changes the signature when a file is touched", () => {
    const touched = makeWorkspaceTree();
    findNode(touched, "scripts/farm.py")!.modifiedMs = 2000;
    expect(treeSignature(touched)).not.toBe(treeSignature(tree));
    expect(treeSignature(makeWorkspaceTree())).toBe(treeSignature(tree));
  });
});
