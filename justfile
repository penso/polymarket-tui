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
# Usage: 
#   just ws-test <polymarket_url>  - Test with a Polymarket event URL
#   just ws-test <asset_id>       - Test with a specific asset ID
#   just ws-test                  - Test with a random active asset
#
# Example: just ws-test https://polymarket.com/event/will-anyone-be-charged-over-daycare-fraud-in-minnesota-by
ws-test input='':
    #!/usr/bin/env bash
    set -euo pipefail
    
    if [ -z "{{input}}" ]; then
        echo "Fetching random active asset ID from API..."
        TOKEN_IDS_JSON=$(curl -s "https://gamma-api.polymarket.com/events?active=true&closed=false&limit=1" | \
            jq -r '.[0].markets[0].clobTokenIds' 2>/dev/null || echo "")
        
        if [ -z "$TOKEN_IDS_JSON" ] || [ "$TOKEN_IDS_JSON" = "null" ]; then
            echo "Error: Could not fetch asset ID."
            echo "Usage: just ws-test <polymarket_url> or just ws-test <asset_id>"
            exit 1
        fi
        
        ASSET_IDS=$(echo "$TOKEN_IDS_JSON" | jq -r '.[]' | tr '\n' ' ')
        ASSET_IDS_ARRAY=$(echo "$TOKEN_IDS_JSON" | jq -r '.')
    elif [[ "{{input}}" =~ ^https?://.*polymarket\.com/event/ ]]; then
        # Extract slug from URL
        SLUG=$(echo "{{input}}" | sed -E 's|.*polymarket\.com/event/([^/?]+).*|\1|')
        echo "Extracting asset IDs from event: $SLUG"
        
        # Fetch event by slug
        EVENT_DATA=$(curl -s "https://gamma-api.polymarket.com/events?slug=$SLUG&active=true" 2>/dev/null)
        
        if [ -z "$EVENT_DATA" ] || [ "$EVENT_DATA" = "null" ] || [ "$EVENT_DATA" = "[]" ]; then
            echo "Error: Could not find event with slug: $SLUG"
            exit 1
        fi
        
        # Get all asset IDs from all markets in the event
        # clobTokenIds is a JSON string, so we need to parse each one and combine
        ALL_ASSET_IDS=""
        while IFS= read -r token_ids_json; do
            if [ -n "$token_ids_json" ] && [ "$token_ids_json" != "null" ]; then
                # Parse the JSON string and extract asset IDs
                IDs=$(echo "$token_ids_json" | jq -r '.[]' 2>/dev/null)
                if [ -n "$IDs" ]; then
                    ALL_ASSET_IDS="$ALL_ASSET_IDS $IDs"
                fi
            fi
        done < <(echo "$EVENT_DATA" | jq -r '.[0].markets[]?.clobTokenIds // empty')
        
        # Convert to JSON array, removing duplicates and empty entries
        ASSET_IDS_ARRAY=$(echo "$ALL_ASSET_IDS" | tr ' ' '\n' | grep -v '^$' | sort -u | jq -R -s 'split("\n") | map(select(length > 0))' 2>/dev/null || echo "[]")
        
        if [ "$ASSET_IDS_ARRAY" = "[]" ] || [ -z "$ASSET_IDS_ARRAY" ]; then
            echo "Error: No active markets found for this event."
            exit 1
        fi
        
        ASSET_COUNT=$(echo "$ASSET_IDS_ARRAY" | jq 'length')
        echo "Found $ASSET_COUNT asset ID(s) for this event"
    else
        # Assume it's an asset ID
        ASSET_IDS_ARRAY="[\"{{input}}\"]"
        echo "Using provided asset ID: {{input}}"
    fi
    
    echo ""
    echo "Asset IDs to subscribe to:"
    echo "$ASSET_IDS_ARRAY" | jq .
    echo ""
    
    # Create subscription message with all asset IDs
    SUB_MSG=$(jq -n \
        --arg type "market" \
        --argjson assets_ids "$ASSET_IDS_ARRAY" \
        '{type: $type, assets_ids: $assets_ids}')
    
    echo "Subscription message:"
    echo "$SUB_MSG" | jq .
    echo ""
    echo "Connecting to Polymarket WebSocket..."
    echo "Waiting for messages (this may take a few seconds)..."
    echo "Press Ctrl+C to exit"
    echo ""
    
    # Connect and send subscription, then listen
    (echo "$SUB_MSG"; sleep 3600) | websocat -t "wss://ws-subscriptions-clob.polymarket.com/ws/market" 2>&1 || \
        (echo "Connection failed. Make sure websocat is installed: brew install websocat" && exit 1)

