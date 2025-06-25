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

# Function to take a screenshot
take_screenshot() {
    local name=$1
    local filename="${SCREENSHOT_DIR}/${name}.png"
    
    echo "Taking screenshot: $name"
    
    # Focus the app window first
    if focus_app; then
        sleep 0.5
        # Use ImageMagick import to capture the focused window
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

# Function to navigate using relative window positioning
click_menu_item() {
    local item_name=$1
    focus_app
    
    # Get window position and size
    # Find the first window matching the app name
    window_id=$(xdotool search --name "$APP_NAME" | head -1)

    # If a window was found, get its geometry
    if [ -n "$window_id" ]; then
        eval $(xdotool getwindowgeometry --shell "$window_id")
        # echo "Window ID: $window_id"
        echo "Position: $X,$Y"
        echo "Size: $WIDTH x $HEIGHT"
    else
        echo "No window found with name: $APP_NAME"
    fi

    # Calculate left panel icon positions (based on UI analysis)
    # Left panel is typically 60px wide with icons spaced ~50px apart vertically
    local left_panel_x=$((X + 45))  # Center of left panel
    local start_y=$((Y + 80))       # Start of icon area
    local ICON_VERTICAL_SPACING=75
    
    case $item_name in
        "identities")
            # Identities (1st icon)
            xdotool mousemove $left_panel_x $((start_y + 0)) click 1
            ;;
        "contracts")
            # Contracts (2nd icon)  
            xdotool mousemove $left_panel_x $((start_y + (ICON_VERTICAL_SPACING * 1))) click 1
            ;;
        "tokens")
            # Tokens (3rd icon)
            xdotool mousemove $left_panel_x $((start_y + (ICON_VERTICAL_SPACING * 2))) click 1
            ;;
        "dpns")
            # DPNS (4th icon)
            xdotool mousemove $left_panel_x $((start_y + (ICON_VERTICAL_SPACING * 3))) click 1
            ;;
        "wallets")
            # Wallets (5th icon)
            xdotool mousemove $left_panel_x $((start_y + (ICON_VERTICAL_SPACING * 4))) click 1
            ;;
        "tools")
            # Tools/Proof Log (6th icon)
            xdotool mousemove $left_panel_x $((start_y + (ICON_VERTICAL_SPACING * 5))) click 1
            ;;
        "network")
            # Network Chooser (7th icon)
            xdotool mousemove $left_panel_x $((start_y + (ICON_VERTICAL_SPACING * 6))) click 1
            ;;
    esac
    
    sleep $DELAY
}

# Wait for application to be ready
echo "Waiting for $APP_NAME to be ready..."
sleep 3

if ! focus_app; then
    echo "Error: Could not find $APP_NAME window. Make sure the application is running."
    exit 1
fi

echo "Starting automated screenshot sequence..."

# Main screen
# take_screenshot "01_main_screen"

# Navigate through different screens using automation
echo "Capturing Identities screen..."
click_menu_item "identities"
take_screenshot "01_identities_screen"

echo "Capturing Contract screen..."
click_menu_item "contracts"
take_screenshot "02_contract_screen"

echo "Capturing Token Balances screen..."
click_menu_item "tokens"
take_screenshot "03_token_balances_screen"

echo "Capturing DPNS screen..."
click_menu_item "dpns"
take_screenshot "04_dpns_contests_screen"

echo "Capturing Wallets screen..."
click_menu_item "wallets"
take_screenshot "05_wallets_screen"

echo "Capturing Tools screen..."
click_menu_item "tools"
take_screenshot "06_tools_screen"

echo "Capturing Network Chooser screen..."
click_menu_item "network"
take_screenshot "07_network_chooser_screen"

# # Try to capture some dialog examples
# echo "Attempting to capture dialog examples..."
# focus_app
# xdotool key Escape  # Close any open dialogs
# sleep 1

xdotool key Escape
sleep 1

echo "Screenshot capture complete!"
echo "Screenshots saved in: $SCREENSHOT_DIR"
ls -la "$SCREENSHOT_DIR"

echo ""
echo "Note: Some screenshots may need manual adjustment of click coordinates"
echo "based on the actual UI layout of the application."