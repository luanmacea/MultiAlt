import type { FsNode } from "../components/python/pythonApi";

export function file(path: string, overrides: Partial<FsNode> = {}): FsNode {
  return {
    path,
    name: path.slice(path.lastIndexOf("/") + 1),
    kind: "file",
    size: 120,
    modifiedMs: 1000,
    writable: true,
    pinned: false,
    runnable: false,
    children: null,
    ...overrides,
  };
}

export function dir(path: string, children: FsNode[], overrides: Partial<FsNode> = {}): FsNode {
  return {
    path,
    name: path.slice(path.lastIndexOf("/") + 1),
    kind: "dir",
    size: 0,
    modifiedMs: 1000,
    writable: true,
    pinned: false,
    runnable: false,
    children,
    ...overrides,
  };
}

/** The workspace the app seeds: scripts, lib, assets, examples and the pinned blocks. */
export function makeWorkspaceTree(options: { developerMode?: boolean } = {}): FsNode {
  const pinned = (name: string) =>
    file(`multialt/defaults/${name}`, { pinned: true, writable: options.developerMode === true });
  return dir(
    "",
    [
      dir("scripts", [
        file("scripts/example_walk_jump.py", { runnable: true }),
        file("scripts/farm.py", { runnable: true }),
        dir("scripts/tools", [file("scripts/tools/util.py")]),
      ]),
      dir("lib", [file("lib/helpers.py")]),
      dir("assets", [file("assets/play_button.png")]),
      dir("examples", [file("examples/walk_and_jump.py", { writable: false })], { writable: false }),
      dir(
        "multialt",
        [
          dir(
            "multialt/defaults",
            ["runner.py", "focus.py", "vision.py", "actions.py", "dashboard.py"].map(pinned),
            { writable: false }
          ),
        ],
        { writable: false }
      ),
    ],
    { name: "" }
  );
}
