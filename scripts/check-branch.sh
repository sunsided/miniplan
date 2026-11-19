#!/bin/sh
# Check a branch name against the repository's branching model.
#
#   sh scripts/check-branch.sh [branch] [github|gitflow]
#
# branch defaults to the current branch; the model defaults to $BRANCH_FLOW or "github".
#   github:  main, feature/*, hotfix/*, release/*                   (GitHub Flow)
#   gitflow: main, develop, feature/*, release/*, hotfix/*, support/* (Git Flow)
# release/* and hotfix/* are release branches in both models (GitVersion: -beta builds) and
# release/* also carries release-plz PRs. dependabot/* is allowed because it cannot be renamed.
# copilot/* (Copilot cloud agent) only warns: migrate it to feature/* before working on it.
set -eu

branch="${1:-$(git rev-parse --abbrev-ref HEAD)}"
flow="${2:-${BRANCH_FLOW:-github}}"

case "$flow:$branch" in
  *:main | *:feature/?* | *:release/?* | *:hotfix/?* | *:dependabot/?*) exit 0 ;;
  gitflow:develop | gitflow:support/?*) exit 0 ;;
  *:copilot/?*)
    echo "warning: '$branch' is a Copilot branch; migrate it to feature/${branch#copilot/} before working on it"
    exit 0
    ;;
  *:HEAD) exit 0 ;; # detached HEAD (rebase, bisect): nothing to check
esac

if [ "$flow" = gitflow ]; then
  allowed="main, develop, feature/*, release/*, hotfix/*, support/*"
else
  allowed="main, feature/*, hotfix/*, release/*"
fi

echo "error: branch '$branch' does not follow the $flow branching model (allowed: $allowed)"
echo "       rename it: git branch -m feature/<topic>   (never chore/, fix/, deps/, ...)"
exit 1
