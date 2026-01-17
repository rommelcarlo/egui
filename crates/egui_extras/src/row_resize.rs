//! Row height resizing support for egui_extras tables.
//!
//! This module provides functionality to allow users to resize row heights
//! by dragging row borders, similar to column resizing.

use std::collections::HashMap;
use egui::{Id, Pos2, Rangef, Rect, Ui, Vec2};

/// State for tracking resized row heights.
/// Uses sparse storage - only stores heights that differ from default.
#[derive(Clone, Debug, Default)]
#[cfg_attr(feature = "serde", derive(serde::Deserialize, serde::Serialize))]
pub struct RowResizeState {
    /// Row heights that have been customized (row_index -> height)
    row_heights: HashMap<usize, f32>,
    
    /// Currently dragging row index
    #[cfg_attr(feature = "serde", serde(skip))]
    dragging_row: Option<usize>,
}

impl RowResizeState {
    /// Load row resize state from egui's memory.
    pub fn load(ui: &Ui, state_id: Id) -> Self {
        #[cfg(feature = "serde")]
        {
            ui.data_mut(|d| d.get_persisted::<Self>(state_id)).unwrap_or_default()
        }
        #[cfg(not(feature = "serde"))]
        {
            ui.data_mut(|d| d.get_temp::<Self>(state_id)).unwrap_or_default()
        }
    }
    
    /// Store row resize state to egui's memory.
    pub fn store(&self, ui: &Ui, state_id: Id) {
        #[cfg(feature = "serde")]
        {
            ui.data_mut(|d| d.insert_persisted(state_id, self.clone()));
        }
        #[cfg(not(feature = "serde"))]
        {
            ui.data_mut(|d| d.insert_temp(state_id, self.clone()));
        }
    }
    
    /// Get the height for a specific row, or the default if not customized.
    pub fn get_row_height(&self, row_index: usize, default_height: f32) -> f32 {
        self.row_heights.get(&row_index).copied().unwrap_or(default_height)
    }
    
    /// Set a custom height for a specific row.
    pub fn set_row_height(&mut self, row_index: usize, height: f32) {
        self.row_heights.insert(row_index, height);
    }
    
    /// Reset a row's height to default (removes the custom height).
    pub fn reset_row_height(&mut self, row_index: usize) {
        self.row_heights.remove(&row_index);
    }
    
    /// Reset all row heights to default.
    pub fn reset_all(&mut self) {
        self.row_heights.clear();
    }
    
    /// Check if currently dragging a row border.
    pub fn is_dragging(&self) -> bool {
        self.dragging_row.is_some()
    }
    
    /// Get the currently dragging row index.
    pub fn dragging_row(&self) -> Option<usize> {
        self.dragging_row
    }
}

/// Configuration for row resizing behavior.
#[derive(Clone, Copy, Debug)]
pub struct RowResizeConfig {
    /// Whether row resizing is enabled.
    pub enabled: bool,
    
    /// Default height for rows.
    pub default_height: f32,
    
    /// Minimum and maximum allowed row heights.
    pub height_range: Rangef,
    
    /// How far from the row border the resize handle extends.
    pub grab_radius: f32,
    
    /// Whether to allow row resizing in the table body (not just header).
    /// Default is false - only row headers can be used to resize.
    pub resize_in_body: bool,
}

impl Default for RowResizeConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            default_height: 20.0,
            height_range: Rangef::new(10.0, f32::INFINITY),
            grab_radius: 5.0,
            resize_in_body: false,
        }
    }
}

impl RowResizeConfig {
    /// Create a new config with resizing enabled.
    pub fn enabled() -> Self {
        Self {
            enabled: true,
            ..Default::default()
        }
    }
    
    /// Set the default row height.
    pub fn default_height(mut self, height: f32) -> Self {
        self.default_height = height;
        self
    }
    
    /// Set the allowed height range.
    pub fn height_range(mut self, range: impl Into<Rangef>) -> Self {
        self.height_range = range.into();
        self
    }
    
    /// Enable row resizing in table body (not just row headers).
    /// Default is false - only the row header column can be used to resize rows.
    pub fn resize_in_body(mut self, enable: bool) -> Self {
        self.resize_in_body = enable;
        self
    }
}

/// Handle row border resize interaction.
/// 
/// Call this for each row after rendering it, passing the row's bottom Y position.
/// Set `is_header` to true when calling from the row header column.
/// Returns the new height if the row was resized.
pub fn handle_row_resize(
    ui: &Ui,
    state: &mut RowResizeState,
    config: &RowResizeConfig,
    row_index: usize,
    row_bottom_y: f32,
    left_x: f32,
    right_x: f32,
    state_id: Id,
    is_header: bool,
) -> Option<f32> {
    if !config.enabled {
        return None;
    }
    
    // Only allow resize in header unless resize_in_body is enabled
    if !is_header && !config.resize_in_body {
        return None;
    }
    
    let resize_id = state_id.with("resize_row").with(row_index);
    
    // Calculate the interact rect for this row's bottom border
    let p0 = Pos2::new(left_x, row_bottom_y);
    let p1 = Pos2::new(right_x, row_bottom_y);
    let interact_rect = Rect::from_min_max(p0, p1)
        .expand2(Vec2::new(0.0, config.grab_radius));
    
    // Check if pointer is in the resize rect (in screen coordinates)
    let pointer_pos = ui.ctx().input(|i| i.pointer.hover_pos());
    let pointer_in_rect = pointer_pos.map_or(false, |pos| interact_rect.contains(pos));
    
    // Track drag state
    let drag_key = resize_id.with("row_drag");
    let was_dragging: bool = ui.data(|d| d.get_temp(drag_key).unwrap_or(false));
    
    let primary_down = ui.ctx().input(|i| i.pointer.primary_down());
    let primary_pressed = ui.ctx().input(|i| i.pointer.primary_pressed());
    
    // Start drag on press in rect
    let is_dragging = if primary_pressed && pointer_in_rect {
        state.dragging_row = Some(row_index);
        true
    } else if was_dragging && primary_down {
        true
    } else {
        if was_dragging {
            state.dragging_row = None;
        }
        false
    };
    
    ui.data_mut(|d| d.insert_temp(drag_key, is_dragging));
    
    let mut new_height = None;
    
    // Handle drag
    if is_dragging {
        let drag_delta = ui.ctx().input(|i| i.pointer.delta());
        let current_height = state.get_row_height(row_index, config.default_height);
        let updated_height = config.height_range.clamp(current_height + drag_delta.y);
        
        if (updated_height - current_height).abs() > 0.01 {
            state.set_row_height(row_index, updated_height);
            new_height = Some(updated_height);
        }
        
        ui.ctx().set_cursor_icon(egui::CursorIcon::ResizeRow);
    } else if pointer_in_rect {
        let dragging_something_else = ui.input(|i| i.pointer.any_down());
        if !dragging_something_else {
            ui.ctx().set_cursor_icon(egui::CursorIcon::ResizeRow);
        }
    }
    
    new_height
}
