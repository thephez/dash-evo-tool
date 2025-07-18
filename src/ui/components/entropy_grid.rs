use bip39::rand::{self, Rng};
use egui::{Button, Color32, Grid, Ui, Vec2};

pub struct U256EntropyGrid {
    random_number: [u8; 32], // Current 256-bit number (32 bytes)
    last_bit_changed: u8,    // Store the last bit position changed
}

impl Default for U256EntropyGrid {
    fn default() -> Self {
        Self::new()
    }
}

impl U256EntropyGrid {
    /// Create a new instance with a random [u8; 32] number
    pub fn new() -> Self {
        let mut rng = rand::thread_rng();
        let mut random_number = [0u8; 32];
        rng.fill(&mut random_number); // Fill with random bytes

        Self {
            random_number,
            last_bit_changed: 0, // Initialize to 0
        }
    }

    /// Render the UI and allow users to modify bits
    pub fn ui(&mut self, ui: &mut Ui) -> [u8; 32] {
        ui.heading("1. Hover over this view to create extra randomness for the seed phrase.");

        // Add padding around the grid
        ui.add_space(10.0); // Top padding

        // Calculate button size - make it fixed size for consistency
        let max_grid_width = 400.0; // Maximum width for the entropy grid
        let available_width = ui.available_width().min(max_grid_width) - 20.0; // Account for padding
        let columns = 32usize; // Reduced from 64 for better usability
        let rows = 8usize; // Increased from 4 to maintain 256 bits
        let max_height = 160; // Adjusted for 8 rows
        let button_size = Vec2::new(
            available_width / columns as f32, // Divide the width into columns
            (max_height / rows as i32).min(available_width as i32 / columns as i32) as f32, // Ensure height stays within limit
        );

        // Create a grid with 8 rows and 32 columns (256 bits total).
        ui.horizontal(|ui| {
            ui.add_space(10.0); // Left padding

            Grid::new("entropy_grid")
                .num_columns(columns) // columns, each representing a bit.
                .spacing(Vec2::new(0.0, 0.0)) // No spacing for compact layout.
                .min_col_width(0.0) // Allow columns to shrink without restriction.
                .show(ui, |ui| {
                    for row in 0..rows {
                        for col in 0..columns {
                            let bit_position = (row * columns + col) as u8;
                            let byte_index = (bit_position / 8) as usize;
                            let bit_in_byte = (bit_position % 8) as usize;

                            // Determine the bit value (1 = Black, 0 = White).
                            let bit_value =
                                (self.random_number[byte_index] >> bit_in_byte) & 1 == 1;
                            let color = if bit_value {
                                Color32::BLACK
                            } else {
                                Color32::WHITE
                            };

                            // Create a button with the appropriate size and color.
                            let button = Button::new("").fill(color).min_size(button_size); // Adjust size dynamically.

                            // Render the button and handle interactions.
                            let response = ui.add(button);

                            if response.hovered() && self.was_bit_different(bit_position)
                                || response.clicked()
                            {
                                self.toggle_bit(byte_index, bit_in_byte); // Toggle the bit.
                            }
                        }
                        ui.end_row(); // Move to the next row after columns bits.
                    }
                });

            ui.add_space(10.0); // Right padding
        });

        ui.add_space(10.0); // Bottom padding

        // Display the current random number in hex.
        ui.label(format!(
            "User number is [{}], this will be added to a random number to add extra entropy and ensure security.",
            hex::encode(self.random_number)
        ));

        self.random_number
    }

    /// Check if the bit at the given position is the same as the last changed bit
    fn was_bit_different(&self, bit_position: u8) -> bool {
        self.last_bit_changed != bit_position
    }

    /// Toggle the bit at the given byte and bit position
    fn toggle_bit(&mut self, byte_index: usize, bit_in_byte: usize) {
        // Toggle the bit using XOR
        self.random_number[byte_index] ^= 1 << bit_in_byte;

        // Update the last changed bit position
        self.last_bit_changed = (byte_index * 8 + bit_in_byte) as u8;
    }

    /// Generate a new random number and XOR it with the current number
    pub fn random_number_with_user_input(&self) -> [u8; 32] {
        let mut rng = rand::thread_rng();
        let mut new_random_number = [0u8; 32];
        rng.fill(&mut new_random_number); // Generate a new random number

        // XOR the new random number with the existing one
        let mut result = [0u8; 32];
        for i in 0..32 {
            result[i] = self.random_number[i] ^ new_random_number[i];
        }
        result
    }
}
