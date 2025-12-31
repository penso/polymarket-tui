# Default recipe (runs when just is called without arguments)
default:
    @just --list

# Format Rust code
format:
    cargo fmt

# Check if code is formatted
format-check:
    cargo fmt -- --check

# Lint Rust code using clippy
lint:
    cargo clippy --all-targets --all-features -- -D warnings

# Build the project
build:
    cargo build

# Build in release mode
build-release:
    cargo build --release

# Test WebSocket connection using websocat
# Usage: just ws-test [asset_id]
# If no asset_id is provided, fetches a random active one
ws-test asset_id='':
    #!/usr/bin/env bash
    set -euo pipefail
    
    if [ -z "{{asset_id}}" ]; then
        echo "Fetching active asset ID from API..."
        # clobTokenIds is a JSON string, so we need to parse it
        TOKEN_IDS_JSON=$(curl -s "https://gamma-api.polymarket.com/events?active=true&closed=false&limit=1" | \
            jq -r '.[0].markets[0].clobTokenIds' 2>/dev/null || echo "")
        
        if [ -z "$TOKEN_IDS_JSON" ] || [ "$TOKEN_IDS_JSON" = "null" ]; then
            echo "Error: Could not fetch asset ID. Please provide one manually:"
            echo "  just ws-test <asset_id>"
            exit 1
        fi
        
        # Parse the JSON string array and get first element
        ASSET_ID=$(echo "$TOKEN_IDS_JSON" | jq -r '.[0]' 2>/dev/null || echo "")
        
        if [ -z "$ASSET_ID" ] || [ "$ASSET_ID" = "null" ]; then
            echo "Error: Could not parse asset ID. Please provide one manually:"
            echo "  just ws-test <asset_id>"
            exit 1
        fi
        echo "Using asset ID: $ASSET_ID"
    else
        ASSET_ID="{{asset_id}}"
    fi
    
    echo "Connecting to Polymarket WebSocket..."
    echo "Subscribing to asset: $ASSET_ID"
    echo "Press Ctrl+C to exit"
    echo ""
    
    # Create subscription message
    SUB_MSG=$(jq -n \
        --arg type "market" \
        --arg asset_id "$ASSET_ID" \
        '{type: $type, assets_ids: [$asset_id]}')
    
    echo "Subscription message:"
    echo "$SUB_MSG" | jq .
    echo ""
    
    # Connect and send subscription, then listen
    # Use websocat with text mode and keep connection open
    echo "Waiting for messages (this may take a few seconds)..."
    echo "You should see incoming WebSocket messages below:"
    echo ""
    (echo "$SUB_MSG"; sleep 3600) | websocat -t "wss://ws-subscriptions-clob.polymarket.com/ws/market" 2>&1 || \
        (echo "Connection failed. Make sure websocat is installed: brew install websocat" && exit 1)

