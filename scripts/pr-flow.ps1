param(
    [string]$Base = "v4",
    [string]$Remote = ""
)

if (-not (Get-Command gh -ErrorAction SilentlyContinue)) {
    throw "GitHub CLI (gh) is required. Install it and run 'gh auth login' first."
}

if (-not (Get-Command git -ErrorAction SilentlyContinue)) {
    throw "Git is required."
}

$availableRemotes = git remote
if (-not $Remote) {
    if ($availableRemotes -contains "fork") {
        $Remote = "fork"
    } elseif ($availableRemotes -contains "origin") {
        $Remote = "origin"
    } else {
        throw "No git remote found. Add a remote before running this script."
    }
}

$currentBranch = (git rev-parse --abbrev-ref HEAD).Trim()

Write-Host ""
Write-Host "Select PR flow:"
Write-Host "1) Add this work to an existing PR"
Write-Host "2) Create a new PR"
$choice = (Read-Host "Choice (1 or 2)").Trim()

switch ($choice) {
    "1" {
        $prNumber = (Read-Host "PR number").Trim()
        if (-not $prNumber) {
            throw "PR number is required."
        }

        $pr = gh pr view $prNumber --json headRefName,baseRefName,url,title | ConvertFrom-Json
        $targetBranch = $pr.headRefName

        if ($currentBranch -ne $targetBranch) {
            git fetch $Remote $targetBranch
            git checkout $targetBranch
            $currentBranch = $targetBranch
        }

        git push -u $Remote $currentBranch

        Write-Host ""
        Write-Host "Updated PR #${prNumber}: $($pr.url)"
    }
    "2" {
        if ($currentBranch -eq $Base) {
            $newBranch = (Read-Host "Current branch is '$Base'. Enter a new branch name").Trim()
            if (-not $newBranch) {
                throw "Branch name is required when starting from base branch."
            }

            git checkout -b $newBranch
            $currentBranch = $newBranch
        }

        git push -u $Remote $currentBranch

        $defaultTitle = (git log -1 --pretty=%s).Trim()
        $title = (Read-Host "PR title [$defaultTitle]").Trim()
        if (-not $title) {
            $title = $defaultTitle
        }

        $body = (Read-Host "PR body (optional)").Trim()
        if (-not $body) {
            $body = "Automated PR flow via scripts/pr-flow.ps1"
        }

        gh pr create --base $Base --head $currentBranch --title $title --body $body
    }
    default {
        throw "Invalid choice. Use 1 or 2."
    }
}
