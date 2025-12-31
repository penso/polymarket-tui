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
#   just ws-test <polymarket_url>           - Test with a Polymarket event URL (all markets)
#   just ws-test <polymarket_url> --first   - Test with only the first market
#   just ws-test <asset_id>                 - Test with a specific asset ID
#   just ws-test                            - Test with a random active asset
#
# Note: A single Polymarket event can have multiple markets (e.g., different end dates).
# Each market has 2 asset IDs (Yes/No outcomes). Use --first to subscribe to just one market.
#
# Example: just ws-test https://polymarket.com/event/will-anyone-be-charged-over-daycare-fraud-in-minnesota-by
# Example: just ws-test https://polymarket.com/event/will-anyone-be-charged-over-daycare-fraud-in-minnesota-by --first
ws-test input='' first='':
    #!/usr/bin/env bash
    set -euo pipefail

    if [ -z "{{input}}" ]; then
        echo "Fetching random active event from API..."
        EVENT_DATA=$(curl -s "https://gamma-api.polymarket.com/events?active=true&closed=false&limit=1" 2>/dev/null)

        if [ -z "$EVENT_DATA" ] || [ "$EVENT_DATA" = "null" ] || [ "$EVENT_DATA" = "[]" ]; then
            echo "Error: Could not fetch event."
            echo "Usage: just ws-test <polymarket_url> or just ws-test <asset_id>"
            exit 1
        fi

        EVENT_TITLE=$(echo "$EVENT_DATA" | jq -r '.[0].title // "Unknown"')
        MARKET_COUNT=$(echo "$EVENT_DATA" | jq '.[0].markets | length')
        echo "Found event: $EVENT_TITLE"
        echo "Found $MARKET_COUNT market(s) in this event"

        # Get all asset IDs from all markets
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

        # Get asset IDs from markets in the event
        # clobTokenIds is a JSON string, so we need to parse each one
        if [ -n "{{first}}" ]; then
            # Only get asset IDs from the first market
            echo "Using only the first market from this event"
            FIRST_MARKET_TOKEN_IDS=$(echo "$EVENT_DATA" | jq -r '.[0].markets[0]?.clobTokenIds // empty')
            if [ -z "$FIRST_MARKET_TOKEN_IDS" ] || [ "$FIRST_MARKET_TOKEN_IDS" = "null" ]; then
                echo "Error: No markets found for this event."
                exit 1
            fi
            ASSET_IDS_ARRAY=$(echo "$FIRST_MARKET_TOKEN_IDS" | jq -r '.')
            MARKET_INFO=$(echo "$EVENT_DATA" | jq -r '.[0].markets[0]?.question // "Unknown"')
            echo "Market: $MARKET_INFO"
        else
            # Get all asset IDs from all markets
            echo "This event has multiple markets. Getting asset IDs from all markets..."
            MARKET_COUNT=$(echo "$EVENT_DATA" | jq '.[0].markets | length')
            echo "Found $MARKET_COUNT market(s) in this event:"
            echo "$EVENT_DATA" | jq -r '.[0].markets[] | "  - \(.question)"'
            echo ""

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
        fi

        if [ "$ASSET_IDS_ARRAY" = "[]" ] || [ -z "$ASSET_IDS_ARRAY" ]; then
            echo "Error: No active markets found for this event."
            exit 1
        fi

        ASSET_COUNT=$(echo "$ASSET_IDS_ARRAY" | jq 'length')
        echo "Subscribing to $ASSET_COUNT asset ID(s)"
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

    # Connect to RTDS WebSocket (Real-Time Data Stream) for activity/trades
    # This is what the Polymarket website uses for showing live trades
    echo "Using RTDS WebSocket for activity monitoring..."
    echo ""

    # Extract event slug if URL was provided
    EVENT_SLUG=""
    if [[ "{{input}}" =~ ^https?://.*polymarket\.com/event/ ]]; then
        EVENT_SLUG=$(echo "{{input}}" | sed -E 's|.*polymarket\.com/event/([^/?]+).*|\1|')
    elif [ -z "{{input}}" ]; then
        # Get event slug from the random event we fetched
        EVENT_SLUG=$(echo "$EVENT_DATA" | jq -r '.[0].slug' 2>/dev/null || echo "")
    fi

    if [ -n "$EVENT_SLUG" ]; then
        # Build the filters JSON string properly (compact format)
        FILTERS_JSON=$(jq -nc --arg event_slug "$EVENT_SLUG" '{event_slug: $event_slug}')
        RTDS_SUB_MSG=$(jq -n \
            --arg filters "$FILTERS_JSON" \
            '{
                action: "subscribe",
                subscriptions: [{
                    topic: "activity",
                    type: "orders_matched",
                    filters: $filters
                }]
            }')

        echo "RTDS Subscription message:"
        echo "$RTDS_SUB_MSG" | jq .
        echo ""
        echo "Connecting to RTDS WebSocket..."
        echo "Waiting for trade activity (this may take a few seconds)..."
        echo "Press Ctrl+C to exit"
        echo ""

        # Send subscription message after a small delay to ensure connection is established
        # Use a subshell to send the message, then keep the connection open
        (sleep 0.3; echo "$RTDS_SUB_MSG"; exec cat) | websocat -t "wss://ws-live-data.polymarket.com/" 2>&1 || \
            (echo "Connection failed. Make sure websocat is installed: brew install websocat" && exit 1)
    else
        # Fallback to CLOB WebSocket if no event slug
        echo "Connecting to CLOB WebSocket..."
        (echo "$SUB_MSG"; sleep 3600) | websocat -t "wss://ws-subscriptions-clob.polymarket.com/ws/market" 2>&1 || \
            (echo "Connection failed. Make sure websocat is installed: brew install websocat" && exit 1)
    fi

