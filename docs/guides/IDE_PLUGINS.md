# SynEvid IDE Plugins Guide

**Version**: 1.0
**Date**: 2024-11-24
**Applies To**: VS Code, IntelliJ IDEA, Neovim, Emacs, Zed

## Overview

SynEvid provides IDE integration through Language Server Protocol (LSP) and native extensions.

## VS Code Extension

### Installation

```bash
# From marketplace
code --install-extension synevid.synevid

# Or from VSIX
code --install-extension synevid-0.6.0.vsix
```

### Features

| Feature | Status |
|---------|--------|
| Hover diagnostics | ✅ |
| Go to definition | ✅ |
| Find references | ✅ |
| Impact overlay | ✅ |
| Verify diagnostics | ✅ |
| Impact tree view | ✅ |
| Blast radius panel | ✅ |
| Session timeline | ✅ |
| Configuration UI | ✅ |

### Configuration

```json
// .vscode/settings.json
{
  "synevid.enabled": true,
  "synevid.daemonUrl": "http://127.0.0.1:3333",
  "synevid.loopbackToken": "${env:SYNEVID_LOOPBACK_TOKEN}",
  "synevid.offlineMode": false,
  "synevid.hover.enabled": true,
  "synevid.hover.showImpact": true,
  "synevid.hover.showVerify": true,
  "synevid.diagnostics.enabled": true,
  "synevid.diagnostics.severity": "warning",
  "synevid.treeView.enabled": true,
  "synevid.treeView.showImpact": true,
  "synevid.treeView.showVerify": true,
  "synevid.commands.enabled": true,
  "synevid.offlineMode": false,
  "synevid.daemonPath": "autopsy-daemon",
  "synevid.cliPath": "autopsy",
  "synevid.repoRoot": "${workspaceFolder}"
}
```

### Commands

| Command | Keybinding | Description |
|---------|------------|-------------|
| `synevid.impact` | `Ctrl+Shift+I` | Show impact for symbol at cursor |
| `synevid.verify` | `Ctrl+Shift+V` | Verify file invariants |
| `synevid.baseline` | `Ctrl+Shift+B` | Create baseline |
| `synevid.impactTree` | - | Open impact tree view |
| `synevid.verifyPanel` | - | Open verify panel |
| `synevid.sessionPanel` | - | Open session panel |
| `synevid.blastRadius` | - | Open blast radius view |
| `synevid.daemon.start` | - | Start daemon |
| `synevid.daemon.stop` | - | Stop daemon |
| `synevid.daemon.restart` | - | Restart daemon |

### Hover Integration

Hover over any symbol to see:
- **Impact**: Callers/callees affected
- **Verify**: Invariant status
- **Coverage**: Verified/Partial/Unknown
- **Blast radius**: Organizational impact

```typescript
// Hover provider
export async function provideHover(
  document: TextDocument,
  position: Position,
  token: CancellationToken
): Promise<Hover | null> {
  const symbol = getSymbolAtPosition(document, position);
  if (!symbol) return null;
  
  const [impact, verify] = await Promise.all([
    fetchImpact(symbol),
    fetchVerify(symbol)
  ]);
  
  return new MarkdownString(`
## ${symbol.qualified_name}

### Impact
- **Verdict**: ${impact.verdict}
- **Coverage**: ${impact.coverage}
- **Damaged callers**: ${impact.damaged_callers.length}

### Verification
- **Verdict**: ${verify.verdict}
- **Coverage**: ${verify.coverage}
  `);
}
```

### Impact Tree View

```
Explorer ▼
├── SYNEVID IMPACT
│   ├── Damaged Callers (3)
│   │   ├── OrderService.ts:42 (HIGH)
│   │   ├── PaymentService.ts:15 (MEDIUM)
│   │   └── UserController.ts:8 (LOW)
│   ├── Newly Dead (0)
│   ├── Broken Imports (0)
│   └── Coverage: PARTIAL
│
├── SYNEVID VERIFY
│   ├── Failed (2)
│   │   ├── CLEAN_ARCHITECTURE_LAYERS
│   │   └── NO_CIRCULAR_DEPS
│   ├── Passed (45)
│   └── Coverage: PARTIAL
│
└── SESSION TIMELINE
    ├── 14:30: Baseline created
    ├── 14:32: Impact analysis
    └── 14:35: Session started
```

### Configuration via Command Palette

```
> SynEvid: Configure Daemon
> SynEvid: Set Offline Mode
> SynEvid: Set Daemon URL
> SynEvid: Set Loopback Token
> SynEvid: Clear Cache
> SynEvid: Show Diagnostics
```

## IntelliJ IDEA / PyCharm / WebStorm / Rider

### Plugin Installation

```bash
# From JetBrains Marketplace
# Search: "SynEvid"

# Or install from disk
# Preferences > Plugins > Gear Icon > Install Plugin from Disk...
```

### Features

| Feature | Status |
|---------|--------|
| Inspections (verify) | ✅ |
| Impact intentions | ✅ |
| Tool window | ✅ |
| Run configurations | ✅ |
| Settings UI | ✅ |

### Configuration

```xml
<!-- .idea/synevid.xml -->
<component name="SynEvidSettings">
  <option name="daemonUrl" value="http://127.0.0.1:3333" />
  <option name="loopbackToken" value="" />
  <option name="offlineMode" value="false" />
  <option name="cliPath" value="autopsy" />
  <option name="daemonPath" value="autopsy-daemon" />
  <option name="repoRoot" value="$PROJECT_DIR$" />
  <option name="offlineMode" value="false" />
  <option name="showImpactInEditor" value="true" />
  <option name="showVerifyInEditor" value="true" />
  <option name="showImpactInToolWindow" value="true" />
  <option name="showVerifyInToolWindow" value="true" />
</component>
```

### Run Configurations

```xml
<!-- .idea/runConfigurations/SynEvid_Baseline.xml -->
<configuration name="SynEvid: Baseline" type="Application" factoryName="Application">
  <option name="MAIN_CLASS_NAME" value="com.synevid.cli.Main" />
  <option name="PROGRAM_PARAMETERS" value="baseline --persist" />
  <option name="WORKING_DIRECTORY" value="$PROJECT_DIR$" />
</configuration>

<!-- .idea/runConfigurations/SynEvid_Impact.xml -->
<configuration name="SynEvid: Impact" type="Application" factoryName="Application">
  <option name="MAIN_CLASS_NAME" value="com.synevid.cli.Main" />
  <option name="PROGRAM_PARAMETERS" value="impact -s \$SymbolAtCaret$" />
  <option name="WORKING_DIRECTORY" value="$PROJECT_DIR$" />
</configuration>
```

### Inspections

```xml
<!-- .idea/inspectionProfiles/SynEvid.xml -->
<inspection_tool class="SynEvidVerifyInspection" enabled="true" level="ERROR" />
<inspection_tool class="SynEvidImpactInspection" enabled="true" level="WARNING" />
```

## Neovim (Lua)

### Plugin Installation (lazy.nvim)

```lua
-- lazy.lua
{
  "synevid/nvim-synevid",
  dependencies = {
    "nvim-lua/plenary.nvim",
    "nvim-telescope/telescope.nvim",
  },
  config = function()
    require("synevid").setup({
      daemon_url = "http://127.0.0.1:3333",
      loopback_token = vim.env.SYNEVID_LOOPBACK_TOKEN,
      offline_mode = false,
      cli_path = "autopsy",
      daemon_path = "autopsy-daemon",
      repo_root = vim.fn.getcwd(),
      
      -- UI
      enable_hover = true,
      enable_diagnostics = true,
      enable_tree_view = true,
      hover_timeout = 500,
      
      -- Keymaps
      keymaps = {
        impact = "<leader>si",
        verify = "<leader>sv",
        baseline = "<leader>sb",
        impact_tree = "<leader>st",
        verify_panel = "<leader>sp",
        session_panel = "<leader>ss",
        blast_radius = "<leader>sr",
      },
      
      -- Diagnostics
      diagnostics = {
        enable = true,
        severity = vim.diagnostic.severity.WARN,
        virtual_text = true,
        signs = true,
        underline = true,
      },
    })
  end,
}
```

### Usage

```vim
" Impact at cursor
<leader>si

" Verify file
<leader>sv

" Create baseline
<leader>sb

" Impact tree
<leader>st

" Verify panel
<leader>sp

" Session panel
<leader>ss

" Blast radius
<leader>sr

" Telescope pickers
:Telescope synevid_impact
:Telescope synevid_verify
:Telescope synevid_sessions
```

### Telescope Integration

```lua
-- telescope-synevid.lua
local pickers = require("telescope.pickers")
local finders = require("telescope.finders")
local conf = require("telescope.config").values
local actions = require("telescope.actions")
local action_state = require("telescope.actions.state")

local function synevid_impact(opts)
  opts = opts or {}
  local symbols = get_synevid_symbols()
  
  pickers.new(opts, {
    prompt_title = "SynEvid Impact",
    finder = finders.new_table({
      results = symbols,
      entry_maker = function(entry)
        return {
          value = entry,
          display = string.format("%s (%s)", entry.qualified_name, entry.kind),
          ordinal = entry.qualified_name,
        }
      end,
      sorter = conf.generic_sorter(opts),
      attach_mappings = function(prompt_bufnr, map)
        actions.select_default:replace(function()
          actions.close(prompt_bufnr)
          local selection = action_state.get_selected_entry()
          show_impact(selection.value)
        end)
        return true
      end,
  }):find()
end

return { synevid_impact = synevid_impact }
```

## Emacs (LSP + Custom)

### lsp-mode Configuration

```elisp
;; init.el
(use-package lsp-mode
  :ensure t
  :hook ((typescript-mode js-mode python-mode rust-mode go-mode java-mode) . lsp)
  :config
  (setq lsp-keymap-prefix "C-c l")
  (setq lsp-completion-provider :capf)
  (setq lsp-idle-delay 0.5)
  (setq lsp-enable-snippet nil))

(use-package lsp-ui
  :ensure t
  :config
  (setq lsp-ui-sideline-enable t)
  (setq lsp-ui-sideline-show-hover t)
  (setq lsp-ui-peek-enable t))

;; SynEvid specific
(defun synevid-impact-at-point ()
  (interactive)
  (let ((symbol (lsp-symbol-at-point)))
    (when symbol
      (let ((result (json-read-from-string
                     (shell-command-to-string
                      (format "autopsy impact -s %s --offline --format json" symbol)))))
        (pop-to-buffer (get-buffer-create "*SynEvid Impact*"))
        (erase-buffer)
        (insert (json-pretty-print result))))))
```