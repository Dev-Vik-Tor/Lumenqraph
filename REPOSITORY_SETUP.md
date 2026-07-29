# Repository Setup Instructions

This document contains instructions for configuring the GitHub repository metadata to improve discoverability.

## Required GitHub Repository Settings

These settings should be applied through the GitHub web interface (Settings > General) or via `gh` CLI if you have admin access:

### Description
```
Open-source, self-hostable event indexer for Soroban smart contracts — typed decoding, read layer, webhooks, GraphQL, MCP
```

### Homepage
```
https://lumenqraph.onrender.com
```

### Topics
Add the following topics to the repository (Settings > General > Topics):
- `stellar`
- `soroban`
- `indexer`
- `rust`
- `blockchain`
- `graphql`
- `mcp`
- `web3`

## Applying Settings via gh CLI

If you have admin access to the repository, run:

```bash
# Set description and homepage
gh repo edit --description "Open-source, self-hostable event indexer for Soroban smart contracts — typed decoding, read layer, webhooks, GraphQL, MCP" --homepage "https://lumenqraph.onrender.com"

# Add topics
gh repo edit --add-topic stellar,soroban,indexer,rust,blockchain,graphql,mcp,web3
```

## Applying Settings via GitHub Web Interface

1. Navigate to the repository on GitHub
2. Click "Settings" (requires admin access)
3. Under "General" section:
   - Add the description in the "Description" field
   - Add the homepage URL in the "Website" field
   - Click "Add topic" and add each topic from the list above
4. Click "Save changes"
