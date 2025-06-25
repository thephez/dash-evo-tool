#!/bin/bash

# Fully automated screenshot capture script for Dash Evo Tool documentation
# Uses xdotool and wmctrl for GUI automation

SCREENSHOT_DIR="./screenshots"
DELAY=0.5  # seconds between actions
APP_NAME="Dash Evo Tool v0.9.0-preview.4"

# Create screenshots directory
mkdir -p "$SCREENSHOT_DIR"

echo "Starting fully automated screenshot capture..."

# Function to find and focus the application window
focus_app() {
    local window_id=$(wmctrl -l | grep -i "$APP_NAME" | head -1 | cut -d' ' -f1)
    if [ -n "$window_id" ]; then
        wmctrl -i -a "$window_id"
        sleep 1
        return 0
    else
        echo "Could not find $APP_NAME window"
        return 1
    fi
}

take_screenshot() {
    local name=$1
    local filename="${SCREENSHOT_DIR}/${name}.png"
    echo "Taking screenshot: $name"
    if focus_app; then
        sleep 0.5
        local window_id=$(xdotool search --name "$APP_NAME" | head -1)
        import -window "$window_id" "$filename"
        if [ $? -eq 0 ]; then
            echo "✓ Screenshot saved: $filename"
        else
            echo "✗ Failed to capture: $name"
        fi
    fi
    sleep $DELAY
}

click_ui_element() {
    local zone=$1
    local element=$2
    focus_app

    local window_id=$(xdotool search --name "$APP_NAME" | head -1)
    eval $(xdotool getwindowgeometry --shell "$window_id")

    case "$zone" in
        "left_sidebar")
            local base_x=$((X + 45))
            local base_y=$((Y + 80))
            local vertical_spacing=75
            case "$element" in
                "identities")   local idx=0 ;;
                "contracts")    local idx=1 ;;
                "tokens")       local idx=2 ;;
                "dpns")         local idx=3 ;;
                "wallets")      local idx=4 ;;
                "tools")        local idx=5 ;;
                "network")      local idx=6 ;;
                *) echo "Unknown left_sidebar element: $element"; return 1 ;;
            esac
            local target_x=$base_x
            local target_y=$((base_y + idx * vertical_spacing))
            ;;
        "topbar")
            local topbar_y=$((Y + 10))
            case "$element" in
                "group_actions") local target_x=$((X + 240)) ;;
                "contracts")     local target_x=$((X + 375)) ;;
                "documents")     local target_x=$((X + 505)) ;;
                "add_token")     local target_x=$((X + 950)) ;;
                "refresh")       local target_x=$((X + 1065)) ;;
                # DPNS
                "register_name")     local target_x=$((X + 950)) ;;
                *) echo "Unknown topbar element: $element"; return 1 ;;
            esac
            local target_y=$topbar_y
            ;;
        "screen_sidebar")
            local base_x=$((X + 170))
            local base_y=$((Y + 140))
            local btn_height=35
            local btn_gap=16
            case "$element" in
                "my_tokens")      local idx=0 ;;
                "search_tokens")  local idx=1 ;;
                "token_creator")  local idx=2 ;;
                # DPNS
                "active_contests")  local idx=0 ;;
                "past_contests")  local idx=1 ;;
                "my_usernames")  local idx=2 ;;
                "scheduled_votes")  local idx=3 ;;
                *) echo "Unknown screen_sidebar element: $element"; return 1 ;;
            esac
            local target_x=$base_x
            local target_y=$((base_y + idx * (btn_height + btn_gap)))
            ;;
        *)
            echo "Unknown zone: $zone"
            return 1
            ;;
    esac

    xdotool mousemove $target_x $target_y click 1
    sleep $DELAY
}

echo "Waiting for $APP_NAME to be ready..."
sleep 2

if ! focus_app; then
    echo "Error: Could not find $APP_NAME window. Make sure the application is running."
    exit 1
fi

echo "Starting automated screenshot sequence..."

# Identities screen
click_ui_element "left_sidebar" "identities"
take_screenshot "01_identities_screen"

# Contracts screen
click_ui_element "left_sidebar" "contracts"
take_screenshot "02_contract_screen"

# Tokens screens
click_ui_element "left_sidebar" "tokens"

    # Tokens - My Tokens tab (default)
    click_ui_element "screen_sidebar" "my_tokens"
    take_screenshot "03a_tokens_my_tokens"

    # Tokens - Search Tokens tab
    click_ui_element "screen_sidebar" "search_tokens"
    take_screenshot "03b_tokens_search_tokens"

    # Tokens - Token Creator tab
    click_ui_element "screen_sidebar" "token_creator"
    take_screenshot "03c_tokens_token_creator"

    # Topbar actions (example)
    click_ui_element "left_sidebar" "tokens"
    click_ui_element "topbar" "add_token"
    take_screenshot "03d_tokens_add_token"

# DPNS screen
click_ui_element "left_sidebar" "dpns"

    # DPNS - Register Name
    click_ui_element "topbar" "register_name"
    take_screenshot "04_dpns_register_name"

   # DPNS - Past contestants
    click_ui_element "screen_sidebar" "active_contests"
    take_screenshot "04a_dpns_active_contests"

    # DPNS - Past contestants
    click_ui_element "screen_sidebar" "past_contests"
    take_screenshot "04b_dpns_past_contests"

    # DPNS - Past contestants
    click_ui_element "screen_sidebar" "my_usernames"
    take_screenshot "04c_dpns_my_usernames"

    # DPNS - Past contestants
    click_ui_element "screen_sidebar" "scheduled_votes"
    take_screenshot "04d_dpns_scheduled_votes"

# Wallets screen
click_ui_element "left_sidebar" "wallets"
take_screenshot "05_wallets_screen"

# Tools screen
click_ui_element "left_sidebar" "tools"
take_screenshot "06_tools_screen"

# Network Chooser screen
click_ui_element "left_sidebar" "network"
take_screenshot "07_network_chooser_screen"


xdotool key Escape
sleep 1

echo "Screenshot capture complete!"
echo "Screenshots saved in: $SCREENSHOT_DIR"
ls -la "$SCREENSHOT_DIR"

echo ""
echo "Note: Some screenshots may need manual adjustment of click coordinates"
echo "based on the actual UI layout of the application."
