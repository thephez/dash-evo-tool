#!/bin/bash

# Fully automated screenshot capture script for Dash Evo Tool documentation
# Uses xdotool and wmctrl for GUI automation
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
CUSTOM_CSV="$SCRIPT_DIR/ui_custom.csv"
SCREENSHOT_DIR="$SCRIPT_DIR/screenshots"

DELAY=0.15  # seconds between actions (reduce for faster runs, increase if UI lags)
FOCUS_APP_DELAY=0.2  # seconds to wait after focusing app to avoid race conditions
APP_NAME="Dash Evo Tool v0.9.0-preview.4"

# Create screenshots directory
mkdir -p "$SCREENSHOT_DIR"

echo "Starting fully automated screenshot capture..."

# Function to find and focus the application window
focus_app() {
    local window_id=$(wmctrl -l | grep -i "$APP_NAME" | head -1 | cut -d' ' -f1)
    if [ -n "$window_id" ]; then
        wmctrl -i -a "$window_id"
        sleep $FOCUS_APP_DELAY
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
        sleep $FOCUS_APP_DELAY
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
            local base_x=$((X + 45))   # 45px offset from window left edge to align with sidebar
            local base_y=$((Y + 80))   # 80px offset from window top to align with first sidebar item
            local vertical_spacing=75  # Vertical distance in pixels between sidebar buttons
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
            local topbar_y=$((Y + 10))   # 10px from window top to align with top bar
            case "$element" in
                # target_x is the pixel offset from window left edge
                "group_actions") local target_x=$((X + 850)) ;;  # X+850: group actions button location
                "contracts")     local target_x=$((X + 975)) ;;  # X+975: contracts tab/button
                "documents")     local target_x=$((X + 1090)) ;; # X+1090: documents tab/button
                "add_token")     local target_x=$((X + 950)) ;;  # X+950: add token button
                "refresh")       local target_x=$((X + 1065)) ;; # X+1065: token refresh button
                # DPNS
                "register_name")     local target_x=$((X + 950)) ;; # X+950: DPNS register name button
                *) echo "Unknown topbar element: $element"; return 1 ;;
            esac
            local target_y=$topbar_y
            ;;
        "screen_sidebar")
            local base_x=$((X + 140))   # 140px from window left edge to reach screen sidebar
            local base_y=$((Y + 140))   # 140px from window top to first sidebar button in this zone
            local btn_height=35         # Button height in sidebar
            local btn_gap=16            # Gap between sidebar buttons
            case "$element" in
                "my_tokens")      local idx=0 ;;
                "search_tokens")  local idx=1 ;;
                "token_creator")  local idx=2 ;;
                # DPNS (overlap in idx for other screens)
                "active_contests")  local idx=0 ;;
                "past_contests")  local idx=1 ;;
                "my_usernames")  local idx=2 ;;
                "scheduled_votes")  local idx=3 ;;
                *) echo "Unknown screen_sidebar element: $element"; return 1 ;;
            esac
            local target_x=$base_x
            local target_y=$((base_y + idx * (btn_height + btn_gap)))
            ;;
        "custom")
            # Look up from ui_custom.csv
            if [ ! -f "$CUSTOM_CSV" ]; then
                echo "Custom UI CSV '$CUSTOM_CSV' not found!"
                return 1
            fi
            local line
            line=$(awk -F',' -v e="$element" 'tolower($1) == tolower(e) {print $2","$3}' "$CUSTOM_CSV" | head -1)
            if [[ -z "$line" ]]; then
                echo "No mapping for custom element '$element' in $CUSTOM_CSV"
                return 1
            fi
            local x_offset=$(echo "$line" | cut -d',' -f1)
            local y_offset=$(echo "$line" | cut -d',' -f2)
            local target_x=$((X + x_offset))
            local target_y=$((Y + y_offset))
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
sleep 2   # Wait 2 seconds at the very start for app to launch/stabilize

if ! focus_app; then
    echo "Error: Could not find $APP_NAME window. Make sure the application is running."
    exit 1
fi

echo "Starting automated screenshot sequence..."

# Identities screen
click_ui_element "left_sidebar" "identities"
take_screenshot "01_identities_screen"

# # Contracts screen
# click_ui_element "left_sidebar" "contracts"
# take_screenshot "02_contract_screen"

#     # Contract - Contracts
#     click_ui_element "topbar" "contracts"
#     take_screenshot "02_contract_contracts"

#     # Contract - Documents
#     click_ui_element "topbar" "documents"
#     take_screenshot "02_contract_documents"

#     # Contract - Group Actions
#     click_ui_element "topbar" "group_actions"
#     take_screenshot "02_contract_group_action"

# # Tokens screens
# click_ui_element "left_sidebar" "tokens"

#     # Token - Add token button
#     click_ui_element "topbar" "add_token"
#     take_screenshot "03_tokens_add_token"

#     # Tokens - My Tokens tab (default)
#     click_ui_element "screen_sidebar" "my_tokens"
#     take_screenshot "03a_tokens_my_tokens"

#     # Tokens - Search Tokens tab
#     click_ui_element "screen_sidebar" "search_tokens"
#     take_screenshot "03b_tokens_search_tokens"

#     # Tokens - Token Creator tab
#     click_ui_element "screen_sidebar" "token_creator"
#     take_screenshot "03c_tokens_token_creator"

# # DPNS screen
# click_ui_element "left_sidebar" "dpns"

#     # DPNS - Register Name
#     click_ui_element "topbar" "register_name"
#     take_screenshot "04_dpns_register_name"

#     # DPNS - Past contestants
#     click_ui_element "left_sidebar" "dpns" # Navigate back to DPNS main screen
#     click_ui_element "screen_sidebar" "active_contests"
#     take_screenshot "04a_dpns_active_contests"

#     # DPNS - Past contestants
#     click_ui_element "screen_sidebar" "past_contests"
#     take_screenshot "04b_dpns_past_contests"

#     # DPNS - Past contestants
#     click_ui_element "screen_sidebar" "my_usernames"
#     take_screenshot "04c_dpns_my_usernames"

#     # DPNS - Past contestants
#     click_ui_element "screen_sidebar" "scheduled_votes"
#     take_screenshot "04d_dpns_scheduled_votes"

# # Wallets screen
# click_ui_element "left_sidebar" "wallets"
# take_screenshot "05_wallets_screen"

# # Tools screen
# click_ui_element "left_sidebar" "tools"
# take_screenshot "06_tools_screen"

# Network Chooser screen
click_ui_element "left_sidebar" "network"
take_screenshot "07_network_chooser_screen"

    # Advanced settings dropdown
    click_ui_element "custom" "advanced_network_settings"
    # Scroll down 3 clicks (for scrolling to advanced settings if needed)
    for i in {1..3}; do   # 1..3 = scroll down 3 times (Button 5 = scroll down)
        xdotool click 5
    done
    take_screenshot "07b_network_chooser_advanced_settings"

xdotool key Escape
sleep 1

echo "Screenshot capture complete!"
echo "Screenshots saved in: $SCREENSHOT_DIR"
ls -la "$SCREENSHOT_DIR"

echo ""
echo "Note: Some screenshots may need manual adjustment of click coordinates"
echo "based on the actual UI layout of the application."
