import { describe, expect, it } from "vitest";

import { buildGitGraphLayout, type GitGraphCommit } from "./git-graph";

function commit(hash: string, parents: string[] = []): GitGraphCommit {
  return {
    hash,
    short_hash: hash.slice(0, 7),
    parents,
    author: "Oxide",
    date: "2026-08-27T00:00:00Z",
    subject: hash,
    refs: [],
  };
}

describe("Git Graph layout", () => {
  it("keeps a linear history in the first lane", () => {
    const rows = buildGitGraphLayout([
      commit("c3", ["c2"]),
      commit("c2", ["c1"]),
      commit("c1"),
    ]);

    expect(rows.map((row) => row.lane)).toEqual([0, 0, 0]);
    expect(rows[0].parentLanes).toEqual([0]);
  });

  it("creates separate parent connections for a merge commit", () => {
    const rows = buildGitGraphLayout([
      commit("merge", ["main", "feature"]),
      commit("feature", ["base"]),
      commit("main", ["base"]),
      commit("base"),
    ]);

    expect(rows[0].parentLanes).toEqual([0, 1]);
    expect(rows[1].lane).toBe(1);
    expect(rows[2].lane).toBe(0);
    expect(rows[3].laneCount).toBe(1);
  });
});
