export interface GitGraphCommit {
  hash: string;
  short_hash: string;
  parents: string[];
  author: string;
  date: string;
  subject: string;
  refs: string[];
}

export interface GitGraphRow {
  commit: GitGraphCommit;
  lane: number;
  laneCount: number;
  parentLanes: number[];
  nextLaneCount: number;
}

function uniqueLanes(lanes: string[]): string[] {
  return lanes.filter((hash, index) => lanes.indexOf(hash) === index);
}

/**
 * Creates a compact lane layout from topologically ordered Git commits.
 * Each parent is placed in the next row's lane set, preserving merge edges.
 */
export function buildGitGraphLayout(commits: GitGraphCommit[]): GitGraphRow[] {
  let lanes: string[] = [];

  return commits.map((commit) => {
    let lane = lanes.indexOf(commit.hash);
    if (lane === -1) {
      lane = lanes.length;
      lanes.push(commit.hash);
    }
    const laneCount = lanes.length;
    const nextLanes = lanes.slice();
    nextLanes.splice(lane, 1, ...commit.parents);
    lanes = uniqueLanes(nextLanes);

    return {
      commit,
      lane,
      laneCount,
      parentLanes: commit.parents
        .map((parent) => lanes.indexOf(parent))
        .filter((parentLane) => parentLane >= 0),
      nextLaneCount: lanes.length,
    };
  });
}
