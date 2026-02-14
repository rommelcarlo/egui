//! Table view with (optional) fixed header and scrolling body.
//! Cell widths are precalculated with given size hints so we can have tables like this:
//! | fixed size | all available space/minimum | 30% of available width | fixed size |
//! Takes all available height, so if you want something below the table, put it in a strip.

use egui::{
    Align, Id, NumExt as _, Rangef, Rect, Response, ScrollArea, Ui, Vec2, Vec2b,
    scroll_area::{ScrollAreaOutput, ScrollBarVisibility, ScrollSource},
};

use crate::{StripLayout, layout::CellDirection};

// ----------------------------------------------------------------------------

/// Styling configuration for table appearance.
///
/// All fields are optional. When `None`, the table uses `ui.visuals()` defaults.
#[derive(Clone, Debug, Default)]
pub struct TableStyle {
    /// Background color for selected cells/rows.
    /// Falls back to `ui.visuals().selection.bg_fill`.
    pub selection_bg_color: Option<egui::Color32>,

    /// Border stroke for selection highlight.
    /// Falls back to `ui.visuals().selection.stroke`.
    pub selection_stroke: Option<egui::Stroke>,

    /// Stroke for vertical grid lines.
    /// Falls back to `ui.visuals().widgets.noninteractive.bg_stroke`.
    pub vertical_grid_stroke: Option<egui::Stroke>,

    /// Stroke for horizontal grid lines.
    /// Falls back to `ui.visuals().widgets.noninteractive.bg_stroke`.
    pub horizontal_grid_stroke: Option<egui::Stroke>,

    /// Background color for striped rows.
    /// Falls back to `ui.visuals().faint_bg_color`.
    pub striped_bg_color: Option<egui::Color32>,

    /// Background color for hovered rows.
    /// Falls back to `ui.visuals().widgets.hovered.bg_fill`.
    pub hovered_bg_color: Option<egui::Color32>,

    /// Header background color.
    /// When `None`, header uses default cell background.
    pub header_bg_color: Option<egui::Color32>,
}

impl TableStyle {
    /// Create a new empty style (all defaults).
    pub fn new() -> Self {
        Self::default()
    }

    /// Create an Excel-like style with green selection border.
    ///
    /// - Single cell: Green border, no background
    /// - Range selection: Green border, light gray semi-transparent background
    ///
    /// Note: The distinction between single-cell and range selection
    /// must be handled by the application (e.g., using different styles
    /// or modifying `selection_bg_color` based on selection size).
    pub fn excel_style() -> Self {
        // Excel green color: #217346 (professional dark green)
        let excel_green = egui::Color32::from_rgb(33, 115, 70);

        Self {
            // Green selection border (2px for visibility)
            selection_stroke: Some(egui::Stroke::new(2.0, excel_green)),
            // Very subtle gray background for range selection (works in both themes)
            selection_bg_color: Some(egui::Color32::from_rgba_unmultiplied(128, 128, 128, 25)),
            // Neutral gray grid lines - subtle in light mode, visible in dark mode
            vertical_grid_stroke: Some(egui::Stroke::new(1.0, egui::Color32::from_gray(160))),
            horizontal_grid_stroke: Some(egui::Stroke::new(1.0, egui::Color32::from_gray(160))),
            // No striped rows (Excel default)
            striped_bg_color: None,
            // Very subtle hover effect
            hovered_bg_color: Some(egui::Color32::from_rgba_unmultiplied(128, 128, 128, 15)),
            header_bg_color: None,
        }
    }

    /// Create a Google Sheets-like style.
    ///
    /// - Blue selection border
    /// - Light blue semi-transparent background
    pub fn google_sheets_style() -> Self {
        // Google Blue: #1a73e8 (slightly adjusted for better visibility)
        let google_blue = egui::Color32::from_rgb(26, 115, 232);
        // Very subtle blue background (works in both themes)
        let google_bg = egui::Color32::from_rgba_unmultiplied(26, 115, 232, 20);

        Self {
            selection_stroke: Some(egui::Stroke::new(2.0, google_blue)),
            selection_bg_color: Some(google_bg),
            // Neutral gray grid lines - professional in light mode, visible in dark mode
            vertical_grid_stroke: Some(egui::Stroke::new(1.0, egui::Color32::from_gray(160))),
            horizontal_grid_stroke: Some(egui::Stroke::new(1.0, egui::Color32::from_gray(160))),
            striped_bg_color: None,
            // Very subtle blue hover
            hovered_bg_color: Some(egui::Color32::from_rgba_unmultiplied(26, 115, 232, 12)),
            header_bg_color: None,
        }
    }

    /// Set the selection background color.
    #[inline]
    pub fn selection_bg_color(mut self, color: egui::Color32) -> Self {
        self.selection_bg_color = Some(color);
        self
    }

    /// Set the selection border stroke.
    #[inline]
    pub fn selection_stroke(mut self, stroke: impl Into<egui::Stroke>) -> Self {
        self.selection_stroke = Some(stroke.into());
        self
    }

    /// Set the vertical grid line stroke.
    #[inline]
    pub fn vertical_grid_stroke(mut self, stroke: impl Into<egui::Stroke>) -> Self {
        self.vertical_grid_stroke = Some(stroke.into());
        self
    }

    /// Set the horizontal grid line stroke.
    #[inline]
    pub fn horizontal_grid_stroke(mut self, stroke: impl Into<egui::Stroke>) -> Self {
        self.horizontal_grid_stroke = Some(stroke.into());
        self
    }

    /// Set the striped row background color.
    #[inline]
    pub fn striped_bg_color(mut self, color: egui::Color32) -> Self {
        self.striped_bg_color = Some(color);
        self
    }

    /// Set the hovered row background color.
    #[inline]
    pub fn hovered_bg_color(mut self, color: egui::Color32) -> Self {
        self.hovered_bg_color = Some(color);
        self
    }

    /// Set the header background color.
    #[inline]
    pub fn header_bg_color(mut self, color: egui::Color32) -> Self {
        self.header_bg_color = Some(color);
        self
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ColumnResizeMode {
    Live,
    Deferred,
}

impl Default for ColumnResizeMode {
    fn default() -> Self {
        ColumnResizeMode::Live
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub struct TableResizeInfo {
    pub active: bool,
    pub column: Option<usize>,
    pub preview_x: Option<f32>,
    pub pending_width: Option<f32>,
    pub mode: ColumnResizeMode,
}

pub fn table_resize_info(ui: &Ui, id_salt: impl std::hash::Hash) -> Option<TableResizeInfo> {
    let state_id = ui.id().with(id_salt);
    ui.data(|d| d.get_temp::<TableResizeInfo>(state_id.with("__table_resize_info")))
}

// ----------------------------------------------------------------------------

#[derive(Clone, Copy, Debug, PartialEq)]
enum InitialColumnSize {
    /// Absolute size in points
    Absolute(f32),

    /// Base on content
    Automatic(f32),

    /// Take all available space
    Remainder,
}

/// Specifies the properties of a column, like its width range.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Column {
    initial_width: InitialColumnSize,

    width_range: Rangef,

    /// Clip contents if too narrow?
    clip: bool,

    resizable: Option<bool>,

    /// If set, we should accurately measure the size of this column this frame
    /// so that we can correctly auto-size it. This is done as a `sizing_pass`.
    auto_size_this_frame: bool,

    /// If true, this column will not scroll horizontally.
    fixed: bool,
}

impl Column {
    /// Automatically sized based on content.
    ///
    /// If you have many thousands of rows and are therefore using [`TableBody::rows`]
    /// or [`TableBody::heterogeneous_rows`], then the automatic size will only be based
    /// on the currently visible rows.
    pub fn auto() -> Self {
        Self::auto_with_initial_suggestion(100.0)
    }

    /// Automatically sized.
    ///
    /// The given fallback is a loose suggestion, that may be used to wrap
    /// cell contents, if they contain a wrapping layout.
    /// In most cases though, the given value is ignored.
    pub fn auto_with_initial_suggestion(suggested_width: f32) -> Self {
        Self::new(InitialColumnSize::Automatic(suggested_width))
    }

    /// With this initial width.
    pub fn initial(width: f32) -> Self {
        Self::new(InitialColumnSize::Absolute(width))
    }

    /// Always this exact width, never shrink or grow.
    pub fn exact(width: f32) -> Self {
        Self::new(InitialColumnSize::Absolute(width))
            .range(width..=width)
            .clip(true)
    }

    /// Take all the space remaining after the other columns have
    /// been sized.
    ///
    /// If you have multiple [`Column::remainder`] they all
    /// share the remaining space equally.
    pub fn remainder() -> Self {
        Self::new(InitialColumnSize::Remainder)
    }

    fn new(initial_width: InitialColumnSize) -> Self {
        Self {
            initial_width,
            width_range: Rangef::new(0.0, f32::INFINITY),
            resizable: None,
            clip: false,
            auto_size_this_frame: false,
            fixed: false,
        }
    }

    /// Can this column be resized by dragging the column separator?
    ///
    /// If you don't call this, the fallback value of
    /// [`TableBuilder::resizable`] is used (which by default is `false`).
    #[inline]
    pub fn resizable(mut self, resizable: bool) -> Self {
        self.resizable = Some(resizable);
        self
    }

    /// If `true`: Allow the column to shrink enough to clip the contents.
    /// If `false`: The column will always be wide enough to contain all its content.
    ///
    /// Clipping can make sense if you expect a column to contain a lot of things,
    /// and you don't want it too take up too much space.
    /// If you turn on clipping you should also consider calling [`Self::at_least`].
    ///
    /// Default: `false`.
    #[inline]
    pub fn clip(mut self, clip: bool) -> Self {
        self.clip = clip;
        self
    }

    /// If `true`: This column will be fixed at the start and will not scroll horizontally.
    ///
    /// Default: `false`.
    #[inline]
    pub fn column_fixed(mut self, fixed: bool) -> Self {
        self.fixed = fixed;
        self
    }

    /// Won't shrink below this width (in points).
    ///
    /// Default: 0.0
    #[inline]
    pub fn at_least(mut self, minimum: f32) -> Self {
        self.width_range.min = minimum;
        self
    }

    /// Won't grow above this width (in points).
    ///
    /// Default: [`f32::INFINITY`]
    #[inline]
    pub fn at_most(mut self, maximum: f32) -> Self {
        self.width_range.max = maximum;
        self
    }

    /// Allowed range of movement (in points), if in a resizable [`Table`].
    #[inline]
    pub fn range(mut self, range: impl Into<Rangef>) -> Self {
        self.width_range = range.into();
        self
    }

    /// If set, the column will be automatically sized based on the content this frame.
    ///
    /// Do not set this every frame, just on a specific action.
    #[inline]
    pub fn auto_size_this_frame(mut self, auto_size_this_frame: bool) -> Self {
        self.auto_size_this_frame = auto_size_this_frame;
        self
    }

    fn is_auto(&self) -> bool {
        match self.initial_width {
            InitialColumnSize::Automatic(_) => true,
            InitialColumnSize::Absolute(_) | InitialColumnSize::Remainder => false,
        }
    }
}

fn to_sizing(columns: &[Column]) -> crate::sizing::Sizing {
    use crate::Size;

    let mut sizing = crate::sizing::Sizing::default();
    for column in columns {
        let size = match column.initial_width {
            InitialColumnSize::Absolute(width) => Size::exact(width),
            InitialColumnSize::Automatic(suggested_width) => Size::initial(suggested_width),
            InitialColumnSize::Remainder => Size::remainder(),
        }
        .with_range(column.width_range);
        sizing.add(size);
    }
    sizing
}

// -----------------------------------------------------------------=----------

struct TableScrollOptions {
    vscroll: bool,
    hscroll: bool,
    drag_to_scroll: bool,
    stick_to_bottom: bool,
    scroll_to_row: Option<(usize, Option<Align>)>,
    scroll_offset_y: Option<f32>,
    min_scrolled_height: f32,
    max_scroll_height: f32,
    auto_shrink: Vec2b,
    scroll_bar_visibility: ScrollBarVisibility,
    animated: bool,
}

impl Default for TableScrollOptions {
    fn default() -> Self {
        Self {
            vscroll: true,
            hscroll: false,
            drag_to_scroll: true,
            stick_to_bottom: false,
            scroll_to_row: None,
            scroll_offset_y: None,
            min_scrolled_height: 200.0,
            max_scroll_height: f32::INFINITY,
            auto_shrink: Vec2b::TRUE,
            scroll_bar_visibility: ScrollBarVisibility::VisibleWhenNeeded,
            animated: true,
        }
    }
}

// -----------------------------------------------------------------=----------

/// Builder for a [`Table`] with (optional) fixed header and scrolling body.
///
/// You must pre-allocate all columns with [`Self::column`]/[`Self::columns`].
///
/// If you have multiple [`Table`]:s in the same [`Ui`]
/// you will need to give them unique id:s by with [`Self::id_salt`].
///
/// ### Example
/// ```
/// # egui::__run_test_ui(|ui| {
/// use egui_extras::{TableBuilder, Column};
/// TableBuilder::new(ui)
///     .column(Column::auto().resizable(true))
///     .column(Column::remainder())
///     .header(20.0, |mut header| {
///         header.col(|ui| {
///             ui.heading("First column");
///         });
///         header.col(|ui| {
///             ui.heading("Second column");
///         });
///     })
///     .body(|mut body| {
///         body.row(30.0, |mut row| {
///             row.col(|ui| {
///                 ui.label("Hello");
///             });
///             row.col(|ui| {
///                 ui.button("world!");
///             });
///         });
///     });
/// # });
/// ```
pub struct TableBuilder<'a> {
    ui: &'a mut Ui,
    id_salt: Id,
    columns: Vec<Column>,
    striped: Option<bool>,
    resizable: bool,
    resizable_body: bool,
    resize_mode: ColumnResizeMode,
    cell_layout: egui::Layout,
    scroll_options: TableScrollOptions,
    sense: egui::Sense,
    style: TableStyle,
    scroll_bar_companion: Option<Box<dyn FnOnce(&mut Ui, f32) -> f32 + 'a>>,
}

impl<'a> TableBuilder<'a> {
    pub fn new(ui: &'a mut Ui) -> Self {
        let cell_layout = *ui.layout();
        Self {
            ui,
            id_salt: Id::new("__table_state"),
            columns: Default::default(),
            striped: None,
            resizable: false,
            resizable_body: true,
            resize_mode: ColumnResizeMode::Live,
            cell_layout,
            scroll_options: Default::default(),
            sense: egui::Sense::hover(),
            style: TableStyle::default(),
            scroll_bar_companion: None,
        }
    }

    /// Give this table a unique id within the parent [`Ui`].
    ///
    /// This is required if you have multiple tables in the same [`Ui`].
    #[inline]
    #[deprecated = "Renamed id_salt"]
    pub fn id_source(self, id_salt: impl std::hash::Hash) -> Self {
        self.id_salt(id_salt)
    }

    /// Give this table a unique id within the parent [`Ui`].
    ///
    /// This is required if you have multiple tables in the same [`Ui`].
    #[inline]
    pub fn id_salt(mut self, id_salt: impl std::hash::Hash) -> Self {
        self.id_salt = Id::new(id_salt);
        self
    }

    /// Enable striped row background for improved readability.
    ///
    /// Default is whatever is in [`egui::Visuals::striped`].
    #[inline]
    pub fn striped(mut self, striped: bool) -> Self {
        self.striped = Some(striped);
        self
    }

    /// What should table cells sense for? (default: [`egui::Sense::hover()`]).
    #[inline]
    pub fn sense(mut self, sense: egui::Sense) -> Self {
        self.sense = sense;
        self
    }

    /// Make the columns resizable by dragging.
    ///
    /// You can set this for individual columns with [`Column::resizable`].
    /// [`Self::resizable`] is used as a fallback for any column for which you don't call
    /// [`Column::resizable`].
    ///
    /// If the _last_ column is [`Column::remainder`], then it won't be resizable
    /// (and instead use up the remainder).
    ///
    /// Default is `false`.
    #[inline]
    pub fn resizable(mut self, resizable: bool) -> Self {
        self.resizable = resizable;
        self
    }

    /// Enable resizing in the body (default: `true`).
    ///
    /// If `false`, the resize handles in the body will be replaced by non-interactive dividers,
    /// but columns can still be resized via the header if [`Self::resizable`] is set.
    #[inline]
    pub fn resizable_body(mut self, resizable_body: bool) -> Self {
        self.resizable_body = resizable_body;
        self
    }

    /// Configure how column resizing behaves (default: live).
    #[inline]
    pub fn resize_mode(mut self, resize_mode: ColumnResizeMode) -> Self {
        self.resize_mode = resize_mode;
        self
    }

    /// Enable vertical scrolling in body (default: `true`)
    #[inline]
    pub fn vscroll(mut self, vscroll: bool) -> Self {
        self.scroll_options.vscroll = vscroll;
        self
    }

    /// Enable horizontal scrolling in body (default: `false`)
    #[inline]
    pub fn hscroll(mut self, hscroll: bool) -> Self {
        self.scroll_options.hscroll = hscroll;
        self
    }

    /// Enables scrolling the table's contents using mouse drag (default: `true`).
    ///
    /// See [`ScrollArea::drag_to_scroll`] for more.
    #[inline]
    pub fn drag_to_scroll(mut self, drag_to_scroll: bool) -> Self {
        self.scroll_options.drag_to_scroll = drag_to_scroll;
        self
    }

    /// Should the scroll handle stick to the bottom position even as the content size changes
    /// dynamically? The scroll handle remains stuck until manually changed, and will become stuck
    /// once again when repositioned to the bottom. Default: `false`.
    #[inline]
    pub fn stick_to_bottom(mut self, stick: bool) -> Self {
        self.scroll_options.stick_to_bottom = stick;
        self
    }

    /// Set a row to scroll to.
    ///
    /// `align` specifies if the row should be positioned in the top, center, or bottom of the view
    /// (using [`Align::TOP`], [`Align::Center`] or [`Align::BOTTOM`]).
    /// If `align` is `None`, the table will scroll just enough to bring the cursor into view.
    ///
    /// See also: [`Self::vertical_scroll_offset`].
    #[inline]
    pub fn scroll_to_row(mut self, row: usize, align: Option<Align>) -> Self {
        self.scroll_options.scroll_to_row = Some((row, align));
        self
    }

    /// Set the vertical scroll offset position, in points.
    ///
    /// See also: [`Self::scroll_to_row`].
    #[inline]
    pub fn vertical_scroll_offset(mut self, offset: f32) -> Self {
        self.scroll_options.scroll_offset_y = Some(offset);
        self
    }

    /// The minimum height of a vertical scroll area which requires scroll bars.
    ///
    /// The scroll area will only become smaller than this if the content is smaller than this
    /// (and so we don't require scroll bars).
    ///
    /// Default: `200.0`.
    #[inline]
    pub fn min_scrolled_height(mut self, min_scrolled_height: f32) -> Self {
        self.scroll_options.min_scrolled_height = min_scrolled_height;
        self
    }

    /// Don't make the scroll area higher than this (add scroll-bars instead!).
    ///
    /// In other words: add scroll-bars when this height is reached.
    /// Default: `800.0`.
    #[inline]
    pub fn max_scroll_height(mut self, max_scroll_height: f32) -> Self {
        self.scroll_options.max_scroll_height = max_scroll_height;
        self
    }

    /// For each axis (x,y):
    /// * If true, add blank space outside the table, keeping the table small.
    /// * If false, add blank space inside the table, expanding the table to fit the containing ui.
    ///
    /// Default: `true`.
    ///
    /// See [`ScrollArea::auto_shrink`] for more.
    #[inline]
    pub fn auto_shrink(mut self, auto_shrink: impl Into<Vec2b>) -> Self {
        self.scroll_options.auto_shrink = auto_shrink.into();
        self
    }

    /// Set the visibility of both horizontal and vertical scroll bars.
    ///
    /// With `ScrollBarVisibility::VisibleWhenNeeded` (default), the scroll bar will be visible only when needed.
    #[inline]
    pub fn scroll_bar_visibility(mut self, scroll_bar_visibility: ScrollBarVisibility) -> Self {
        self.scroll_options.scroll_bar_visibility = scroll_bar_visibility;
        self
    }

    /// Should the scroll area animate `scroll_to_*` functions?
    ///
    /// Default: `true`.
    #[inline]
    pub fn animate_scrolling(mut self, animated: bool) -> Self {
        self.scroll_options.animated = animated;
        self
    }

    /// What layout should we use for the individual cells?
    #[inline]
    pub fn cell_layout(mut self, cell_layout: egui::Layout) -> Self {
        self.cell_layout = cell_layout;
        self
    }

    /// Set the table style for customizing colors and strokes.
    #[inline]
    pub fn style(mut self, style: TableStyle) -> Self {
        self.style = style;
        self
    }

    /// Add a companion widget to the horizontal scroll bar.
    ///
    /// The callback receives the UI and the available width for the scroll bar area.
    /// It should return the width consumed by the companion widget.
    /// This is useful for adding tabs or other controls next to the scroll bar.
    pub fn scroll_bar_companion(
        mut self,
        add_contents: impl FnOnce(&mut Ui, f32) -> f32 + 'a,
    ) -> Self {
        self.scroll_bar_companion = Some(Box::new(add_contents));
        self
    }

    /// Allocate space for one column.
    #[inline]
    pub fn column(mut self, column: Column) -> Self {
        self.columns.push(column);
        self
    }

    /// Allocate space for several columns at once.
    #[inline]
    pub fn columns(mut self, column: Column, count: usize) -> Self {
        for _ in 0..count {
            self.columns.push(column);
        }
        self
    }

    fn available_width(&self) -> f32 {
        self.ui.available_rect_before_wrap().width()
            - (self.scroll_options.vscroll as i32 as f32)
                * self.ui.spacing().scroll.allocated_width()
    }

    /// Reset all column widths.
    pub fn reset(&self) {
        let state_id = self.ui.id().with(self.id_salt);
        TableState::reset(self.ui, state_id);
    }

    /// Create a header row which always stays visible and at the top
    pub fn header(self, height: f32, add_header_row: impl FnOnce(TableRow<'_, '_>)) -> Table<'a> {
        let available_width = self.available_width();

        let Self {
            ui,
            id_salt,
            mut columns,
            striped,
            resizable,
            resizable_body,
            resize_mode,
            cell_layout,
            scroll_options,
            sense,
            style,
            scroll_bar_companion,
        } = self;

        for (i, column) in columns.iter_mut().enumerate() {
            let column_resize_id = ui.id().with("resize_column").with(i);
            if let Some(response) = ui.ctx().read_response(column_resize_id)
                && response.double_clicked()
            {
                column.auto_size_this_frame = true;
            }
        }

        let striped = striped.unwrap_or_else(|| ui.visuals().striped);

        let state_id = ui.id().with(id_salt);

        let available_width_for_sizing = if scroll_options.hscroll {
            1_000_000.0
        } else {
            available_width
        };

        let (is_sizing_pass, mut state) = TableState::load(
            ui,
            state_id,
            resizable,
            &columns,
            available_width_for_sizing,
        );

        let mut max_used_widths = vec![0.0; columns.len()];
        let table_top = ui.cursor().top();

        // Wrap header in ScrollArea to sync with body
        let mut scroll_area = egui::ScrollArea::new([scroll_options.hscroll, false])
            .id_salt(state_id.with("__header_scroll_area"))
            .scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::AlwaysHidden)
            .scroll_source(ScrollSource {
                drag: scroll_options.drag_to_scroll,
                ..Default::default()
            });

        // Sync header scroll offset to body if body moved
        scroll_area = scroll_area.horizontal_scroll_offset(state.scroll_offset.x);

        let scroll_area_output = scroll_area.show(ui, |ui| {
            // let mut ui_builder = egui::UiBuilder::new();
            // if is_sizing_pass {
            //     ui_builder = ui_builder.sizing_pass();
            // }
            let mut ui_builder = egui::UiBuilder::new();
            if is_sizing_pass {
                ui_builder = ui_builder.sizing_pass();
            }
            // Set the min width to ensure proper scrolling
            // let total_width: f32 = state.column_widths.iter().sum::<f32>()
            //    + ui.spacing().item_spacing.x * (columns.len() as f32 - 1.0);
            ui.scope_builder(ui_builder, |ui| {
                ui.spacing_mut().item_spacing = egui::Vec2::ZERO;
                let preview_id = state_id.with("__resize_preview");
                let resize_info_id = state_id.with("__table_resize_info");
                let mut resize_preview = ui
                    .data_mut(|d| d.get_temp::<ResizePreviewState>(preview_id))
                    .unwrap_or_default();
                let mut resize_info = TableResizeInfo {
                    mode: resize_mode,
                    ..Default::default()
                };
                // Calculate fixed columns width for header (for clipping) - before mutable borrow
                let mut fixed_columns_width = 0.0;
                for (i, column) in columns.iter().enumerate() {
                    if column.fixed {
                        fixed_columns_width += state.column_widths[i];
                    }
                }

                let mut header_widths = state.column_widths.clone();
                if !resizable_body {
                    let header_top = ui.cursor().top();
                    let header_bottom = header_top + height;
                    let start_x = ui.cursor().min.x;
                    let spacing_x = ui.spacing().item_spacing.x;
                    let mut x = start_x;

                    for (i, column) in columns.iter().enumerate() {
                        let column_is_resizable = column.resizable.unwrap_or(resizable);
                        let width_range = column.width_range;
                        let max_used_width = state.max_used_widths.get(i).copied().unwrap_or(0.0);

                        x += header_widths[i] + spacing_x;

                        if !column_is_resizable {
                            continue;
                        }

                        let resize_x = if column.fixed {
                            x + state.scroll_offset.x
                        } else {
                            x
                        };

                        let p0 = egui::pos2(resize_x, header_top);
                        let p1 = egui::pos2(resize_x, header_bottom);
                        let interact_rect = egui::Rect::from_min_max(p0, p1)
                            .expand(ui.style().interaction.resize_grab_radius_side);

                        if interact_rect.is_positive() {
                            let column_resize_id = state_id.with("resize_column").with(i);
                            let resize_response = ui.interact(
                                interact_rect,
                                column_resize_id,
                                egui::Sense::click_and_drag(),
                            );
                            let pointer_pos = ui.input(|i| i.pointer.hover_pos());

                            if resize_mode == ColumnResizeMode::Live {
                                if resize_response.dragged() {
                                    let mut new_width =
                                        header_widths[i] + resize_response.drag_delta().x;
                                    if !column.clip {
                                        new_width = new_width.at_least(max_used_width);
                                    }
                                    new_width = width_range.clamp(new_width);
                                    header_widths[i] = new_width;
                                    state.column_widths[i] = new_width;
                                }
                            } else {
                                if resize_response.drag_started() {
                                    if let Some(pos) = pointer_pos {
                                        resize_preview.active = true;
                                        resize_preview.column = Some(i);
                                        resize_preview.start_width = header_widths[i];
                                        resize_preview.start_pointer_x = pos.x;
                                        resize_preview.start_handle_x = resize_x;
                                        resize_preview.pending_width = header_widths[i];
                                        resize_preview.preview_x = resize_x;
                                    }
                                }

                                if resize_preview.active && resize_preview.column == Some(i) {
                                    if let Some(pos) = pointer_pos {
                                        let delta = pos.x - resize_preview.start_pointer_x;
                                        let mut new_width =
                                            resize_preview.start_width + delta;
                                        if !column.clip {
                                            new_width = new_width.at_least(max_used_width);
                                        }
                                        resize_preview.pending_width =
                                            width_range.clamp(new_width);
                                        resize_preview.preview_x =
                                            resize_preview.start_handle_x + delta;
                                    }
                                    header_widths[i] = resize_preview.pending_width;
                                }

                                if resize_response.drag_stopped()
                                    && resize_preview.column == Some(i)
                                {
                                    state.column_widths[i] = resize_preview.pending_width;
                                    header_widths[i] = resize_preview.pending_width;
                                    resize_preview = ResizePreviewState::default();
                                }
                            }

                            let dragging_something_else =
                                ui.input(|i| i.pointer.any_down() || i.pointer.any_pressed());
                            let resize_hover =
                                resize_response.hovered() && !dragging_something_else;
                            let drag_active = if resize_mode == ColumnResizeMode::Live {
                                resize_response.dragged()
                            } else {
                                resize_preview.active && resize_preview.column == Some(i)
                            };

                            if resize_hover || drag_active {
                                ui.ctx().set_cursor_icon(egui::CursorIcon::ResizeColumn);
                            }

                            if drag_active {
                                let preview_x = if resize_mode == ColumnResizeMode::Deferred {
                                    resize_preview.preview_x
                                } else {
                                    resize_x
                                };
                                resize_info.active = true;
                                resize_info.column = Some(i);
                                resize_info.preview_x = Some(preview_x);
                                resize_info.pending_width = Some(if resize_mode
                                    == ColumnResizeMode::Deferred
                                {
                                    resize_preview.pending_width
                                } else {
                                    header_widths[i]
                                });
                            }
                        }
                    }
                }

                let mut layout =
                    StripLayout::new(ui, CellDirection::Horizontal, cell_layout, sense);
                let mut response: Option<Response> = None;

                add_header_row(TableRow {
                    layout: &mut layout,
                    columns: &columns,
                    widths: &header_widths,
                    max_used_widths: &mut max_used_widths,
                    row_index: 0,
                    col_index: 0,
                    height,
                    striped: false,
                    hovered: false,
                    selected: false,
                    overline: false,
                    response: &mut response,
                    scroll_offset_x: state.scroll_offset.x,
                    fixed_columns_width,
                    style: style.clone(),
                });
                layout.allocate_rect();

                ui.data_mut(|d| {
                    d.insert_temp(preview_id, resize_preview);
                    d.insert_temp(resize_info_id, resize_info);
                });
            });
        });

        // Update state with new offset (if header was scrolled)
        state.scroll_offset.x = scroll_area_output.state.offset.x;

        let header_bottom = Some(ui.cursor().top());

        Table {
            ui,
            table_top,
            header_bottom,
            state_id,
            columns,
            available_width,
            state,
            max_used_widths,
            is_sizing_pass,
            resizable,
            resizable_body,
            resize_mode,
            striped,
            cell_layout,
            scroll_options,
            sense,
            style,
            scroll_bar_companion,
        }
    }

    /// Create table body without a header row
    pub fn body<F>(self, add_body_contents: F) -> ScrollAreaOutput<()>
    where
        F: for<'b> FnOnce(TableBody<'b>),
    {
        let available_width = self.available_width();

        let Self {
            ui,
            id_salt,
            columns,
            striped,
            resizable,
            resizable_body,
            resize_mode,
            cell_layout,
            scroll_options,
            sense,
            style,
            scroll_bar_companion,
        } = self;

        let striped = striped.unwrap_or_else(|| ui.visuals().striped);

        let state_id = ui.id().with(id_salt);

        let available_width_for_sizing = if scroll_options.hscroll {
            1_000_000.0
        } else {
            available_width
        };

        let (is_sizing_pass, mut state) = TableState::load(
            ui,
            state_id,
            resizable,
            &columns,
            available_width_for_sizing,
        );

        let max_used_widths = vec![0.0; columns.len()];
        let table_top = ui.cursor().top();

        Table {
            ui,
            table_top,
            header_bottom: None,
            state_id,
            columns,
            available_width,
            state,
            max_used_widths,
            is_sizing_pass,
            resizable,
            resizable_body,
            resize_mode,
            striped,
            cell_layout,
            scroll_options,
            sense,
            style,
            scroll_bar_companion,
        }
        .body(add_body_contents)
    }
}

// ----------------------------------------------------------------------------

#[derive(Clone, Debug)]
#[cfg_attr(feature = "serde", derive(serde::Deserialize, serde::Serialize))]
struct TableState {
    column_widths: Vec<f32>,

    /// Current scroll offset (x, y)
    scroll_offset: Vec2,

    /// If known from previous frame
    #[cfg_attr(feature = "serde", serde(skip))]
    max_used_widths: Vec<f32>,
}

#[derive(Clone, Debug, Default)]
struct ResizePreviewState {
    active: bool,
    column: Option<usize>,
    start_width: f32,
    start_pointer_x: f32,
    start_handle_x: f32,
    pending_width: f32,
    preview_x: f32,
}

impl TableState {
    /// Return true if we should do a sizing pass.
    fn load(
        ui: &Ui,
        state_id: egui::Id,
        resizable: bool,
        columns: &[Column],
        available_width: f32,
    ) -> (bool, Self) {
        let rect = Rect::from_min_size(ui.available_rect_before_wrap().min, Vec2::ZERO);
        ui.ctx().check_for_id_clash(state_id, rect, "Table");

        #[cfg(feature = "serde")]
        let state = ui.data_mut(|d| d.get_persisted::<Self>(state_id));
        #[cfg(not(feature = "serde"))]
        let state = ui.data_mut(|d| d.get_temp::<Self>(state_id));

        // Make sure that the stored widths aren't out-dated:
        let state = state.filter(|state| state.column_widths.len() == columns.len());

        let is_sizing_pass =
            ui.is_sizing_pass() || state.is_none() && columns.iter().any(|c| c.is_auto());

        let mut state = state.unwrap_or_else(|| {
            let initial_widths =
                to_sizing(columns).to_lengths(available_width, ui.spacing().item_spacing.x);
            Self {
                column_widths: initial_widths,
                scroll_offset: Vec2::ZERO,
                max_used_widths: Default::default(),
            }
        });

        if !is_sizing_pass && state.max_used_widths.len() == columns.len() {
            // Make sure any non-resizable `remainder` columns are updated
            // to take up the remainder of the current available width.
            // Also handles changing item spacing.
            let mut sizing = crate::sizing::Sizing::default();
            for ((prev_width, max_used), column) in state
                .column_widths
                .iter()
                .zip(&state.max_used_widths)
                .zip(columns)
            {
                use crate::Size;

                let column_resizable = column.resizable.unwrap_or(resizable);
                let size = if column_resizable {
                    // Resiable columns keep their width:
                    Size::exact(*prev_width)
                } else {
                    match column.initial_width {
                        InitialColumnSize::Absolute(width) => Size::exact(width),
                        InitialColumnSize::Automatic(_) => Size::exact(*prev_width),
                        InitialColumnSize::Remainder => Size::remainder(),
                    }
                    .at_least(column.width_range.min.max(*max_used))
                    .at_most(column.width_range.max)
                };
                sizing.add(size);
            }
            state.column_widths = sizing.to_lengths(available_width, ui.spacing().item_spacing.x);
        }

        (is_sizing_pass, state)
    }

    fn store(self, ui: &egui::Ui, state_id: egui::Id) {
        #![expect(clippy::needless_return)]
        #[cfg(feature = "serde")]
        {
            return ui.data_mut(|d| d.insert_persisted(state_id, self));
        }
        #[cfg(not(feature = "serde"))]
        {
            return ui.data_mut(|d| d.insert_temp(state_id, self));
        }
    }

    fn reset(ui: &egui::Ui, state_id: egui::Id) {
        ui.data_mut(|d| d.remove::<Self>(state_id));
    }
}

// ----------------------------------------------------------------------------

/// Table struct which can construct a [`TableBody`].
///
/// Is created by [`TableBuilder`] by either calling [`TableBuilder::body`] or after creating a header row with [`TableBuilder::header`].
pub struct Table<'a> {
    ui: &'a mut Ui,
    table_top: f32,
    header_bottom: Option<f32>,
    state_id: egui::Id,
    columns: Vec<Column>,
    available_width: f32,
    state: TableState,

    /// Accumulated maximum used widths for each column.
    max_used_widths: Vec<f32>,

    /// During the sizing pass we calculate the width of columns with [`Column::auto`].
    is_sizing_pass: bool,
    resizable: bool,
    resizable_body: bool,
    resize_mode: ColumnResizeMode,
    striped: bool,
    cell_layout: egui::Layout,

    scroll_options: TableScrollOptions,

    sense: egui::Sense,

    /// Custom styling options.
    style: TableStyle,
    scroll_bar_companion: Option<Box<dyn FnOnce(&mut Ui, f32) -> f32 + 'a>>,
}

impl Table<'_> {
    /// Access the contained [`egui::Ui`].
    ///
    /// You can use this to e.g. modify the [`egui::Style`] with [`egui::Ui::style_mut`].
    pub fn ui_mut(&mut self) -> &mut egui::Ui {
        self.ui
    }

    /// Create table body after adding a header row
    pub fn body<F>(self, add_body_contents: F) -> ScrollAreaOutput<()>
    where
        F: for<'b> FnOnce(TableBody<'b>),
    {
        let Table {
            ui,
            table_top,
            header_bottom,
            state_id,
            columns,
            resizable,
            resizable_body,
            resize_mode,
            mut available_width,
            mut state,
            mut max_used_widths,
            is_sizing_pass,
            striped,
            cell_layout,
            scroll_options,
            sense,
            style,
            scroll_bar_companion,
        } = self;

        let TableScrollOptions {
            vscroll,
            hscroll,
            drag_to_scroll,
            stick_to_bottom,
            scroll_to_row,
            scroll_offset_y,
            min_scrolled_height,
            max_scroll_height,
            auto_shrink,
            scroll_bar_visibility,
            animated,
        } = scroll_options;

        let cursor_position = ui.cursor().min;

        // Calculate fixed columns width for horizontal scrollbar offset
        let spacing_x = 0.0; // Force zero spacing
        let mut fixed_columns_width_for_scrollbar = 0.0;
        for (i, column) in columns.iter().enumerate() {
            if column.fixed {
                fixed_columns_width_for_scrollbar += state.column_widths[i] + spacing_x;
            }
        }

        let mut companion_width = 0.0;

        if hscroll {
            if let Some(companion) = scroll_bar_companion {
                let available_rect = ui.available_rect_before_wrap();
                let sb_allocated_width = ui.spacing().scroll.allocated_width();
                let bottom = available_rect.bottom();
                let left = available_rect.left();

                let rect = egui::Rect::from_min_size(
                    egui::pos2(left, bottom - sb_allocated_width),
                    egui::vec2(available_rect.width(), sb_allocated_width),
                );

                let mut child_ui =
                    ui.child_ui(rect, egui::Layout::left_to_right(egui::Align::Center), None);
                companion_width = companion(&mut child_ui, available_rect.width());
                println!("[Table::body] companion_width={}", companion_width);
            }
        }

        let mut scroll_area = ScrollArea::new([hscroll, vscroll])
            .id_salt(state_id.with("__scroll_area"))
            .scroll_source(ScrollSource {
                drag: drag_to_scroll,
                ..Default::default()
            })
            .stick_to_bottom(stick_to_bottom)
            .min_scrolled_height(min_scrolled_height)
            .max_height(max_scroll_height)
            .auto_shrink(auto_shrink)
            .scroll_bar_visibility(scroll_bar_visibility)
            .animated(animated);

        if let Some(scroll_offset_y) = scroll_offset_y {
            scroll_area = scroll_area.vertical_scroll_offset(scroll_offset_y);
        }

        // Sync body scroll offset to header if header moved (or if pervious frame had offset)
        scroll_area = scroll_area.horizontal_scroll_offset(state.scroll_offset.x);

        // Offset horizontal scrollbar to start after fixed columns and companion widget
        if (fixed_columns_width_for_scrollbar > 0.0 || companion_width > 0.0) && hscroll {
            let available_rect = ui.available_rect_before_wrap();
            let scrollbar_rect = egui::Rect::from_min_max(
                egui::pos2(
                    available_rect.left() + fixed_columns_width_for_scrollbar + companion_width,
                    available_rect.top(),
                ),
                available_rect.max,
            );
            scroll_area = scroll_area.scroll_bar_rect(scrollbar_rect);
        }

        let columns_ref = &columns;
        // let widths_ref = &state.column_widths; // Removed
        let max_used_widths_ref = &mut max_used_widths;

        let scroll_area_out = scroll_area.show(ui, move |ui| {
            ui.spacing_mut().item_spacing = egui::Vec2::ZERO; // Ensure zero spacing for interaction loops too
            let mut scroll_to_y_range = None;

            let clip_rect = ui.clip_rect();

            let mut ui_builder = egui::UiBuilder::new();
            if is_sizing_pass {
                ui_builder = ui_builder.sizing_pass();
            }

            ui.scope_builder(ui_builder, |ui| {
                ui.spacing_mut().item_spacing = egui::Vec2::ZERO;
                let hovered_row_index_id = self.state_id.with("__table_hovered_row");
                let hovered_row_index =
                    ui.data_mut(|data| data.remove_temp::<usize>(hovered_row_index_id));

                // Calculate fixed columns width for cell clipping - before mutable borrow
                let mut fixed_columns_width = 0.0;
                for (i, column) in columns_ref.iter().enumerate() {
                    if column.fixed {
                        fixed_columns_width += state.column_widths[i];
                    }
                }

                let layout = StripLayout::new(ui, CellDirection::Horizontal, cell_layout, sense);

                add_body_contents(TableBody {
                    layout,
                    columns: columns_ref,
                    widths: &state.column_widths,
                    max_used_widths: max_used_widths_ref,
                    fixed_columns_width,
                    striped,
                    row_index: 0,
                    y_range: clip_rect.y_range(),
                    scroll_to_row: scroll_to_row.map(|(r, _)| r),
                    scroll_to_y_range: &mut scroll_to_y_range,
                    hovered_row_index,
                    hovered_row_index_id,
                    state_id: self.state_id,
                    style: style.clone(),
                });

                if scroll_to_row.is_some() && scroll_to_y_range.is_none() {
                    // TableBody::row didn't find the correct row, so scroll to the bottom:
                    scroll_to_y_range = Some(Rangef::new(f32::INFINITY, f32::INFINITY));
                }
            });

            if let Some(y_range) = scroll_to_y_range {
                let x = 0.0; // ignored, we only have vertical scrolling
                let rect = egui::Rect::from_x_y_ranges(x..=x, y_range);
                let align = scroll_to_row.and_then(|(_, a)| a);
                ui.scroll_to_rect(rect, align);
            }

            // --- GRID LINE LOGIC ---
            let bottom = ui.clip_rect().bottom();
            let top = ui.clip_rect().top();
            let start_x = ui.cursor().min.x;

            let preview_id = state_id.with("__resize_preview");
            let resize_info_id = state_id.with("__table_resize_info");
            let mut resize_preview = ui
                .data_mut(|d| d.get_temp::<ResizePreviewState>(preview_id))
                .unwrap_or_default();
            let mut resize_info = ui
                .data(|d| d.get_temp::<TableResizeInfo>(resize_info_id))
                .unwrap_or_else(|| TableResizeInfo {
                    mode: resize_mode,
                    ..Default::default()
                });
            resize_info.mode = resize_mode;

            let mut fixed_columns_width = 0.0;
            for (i, column) in columns_ref.iter().enumerate() {
                if column.fixed {
                    fixed_columns_width += state.column_widths[i];
                }
            }

            let scrollable_clip_rect = egui::Rect::from_min_max(
                egui::pos2(ui.clip_rect().min.x + fixed_columns_width, top),
                ui.clip_rect().max,
            );

            let header_resize_handled = !resizable_body && header_bottom.is_some();
            let mut x = start_x;
            for (i, column_width) in state.column_widths.iter_mut().enumerate() {
                let column = &columns_ref[i];
                let column_is_resizable = column.resizable.unwrap_or(resizable);
                let width_range = column.width_range;
                let spacing_x = ui.spacing().item_spacing.x;

                if ui.is_sizing_pass() {
                    if column.clip {
                        *column_width = column_width.min(max_used_widths_ref[i]);
                    } else {
                        *column_width = max_used_widths_ref[i];
                    }
                } else if !column.clip {
                    *column_width = column_width.at_least(max_used_widths_ref[i]);
                }
                *column_width = width_range.clamp(*column_width);

                x += *column_width + spacing_x;

                if column.is_auto() && (is_sizing_pass || !column_is_resizable) {
                    *column_width = width_range.clamp(max_used_widths_ref[i]);
                } else if column_is_resizable {
                    let is_fixed = column.fixed;

                    // For fixed columns: counteract scroll offset to appear stationary relative to screen
                    let resize_x = if is_fixed {
                        x + state.scroll_offset.x
                    } else {
                        x
                    };

                    let mut p0 = egui::pos2(resize_x, top);
                    let mut p1 = egui::pos2(resize_x, bottom);

                    let line_rect = egui::Rect::from_min_max(p0, p1)
                        .expand(ui.style().interaction.resize_grab_radius_side);

                    let column_resize_id = state_id.with("resize_column").with(i);

                    if resizable_body {
                        let clip_rect = if is_fixed {
                            ui.clip_rect()
                        } else {
                            scrollable_clip_rect
                        };
                        let valid_rect = line_rect.intersect(clip_rect);

                        if valid_rect.is_positive() {
                            let resize_response = ui.interact(
                                valid_rect,
                                column_resize_id,
                                egui::Sense::click_and_drag(),
                            );

                            let pointer_pos = ui.input(|i| i.pointer.hover_pos());
                            if column.auto_size_this_frame {
                                *column_width = width_range.clamp(max_used_widths_ref[i]);
                            } else if resize_mode == ColumnResizeMode::Live {
                                if resize_response.dragged() {
                                    let mut new_width =
                                        *column_width + resize_response.drag_delta().x;
                                    if !column.clip {
                                        // limit to max_used_widths_ref[i] effectively means we can't shrink below content unless clipped
                                        new_width = new_width.at_least(max_used_widths_ref[i]);
                                    }
                                    new_width = width_range.clamp(new_width);
                                    *column_width = new_width;

                                    // Draw the resize line at the cursor position for immediate visual feedback
                                    if let Some(pointer_pos) = pointer_pos {
                                        let cursor_x = pointer_pos.x;
                                        p0 = egui::pos2(cursor_x, top);
                                        p1 = egui::pos2(cursor_x, bottom);
                                    }
                                }
                            } else {
                                if resize_response.drag_started() {
                                    if let Some(pos) = pointer_pos {
                                        resize_preview.active = true;
                                        resize_preview.column = Some(i);
                                        resize_preview.start_width = *column_width;
                                        resize_preview.start_pointer_x = pos.x;
                                        resize_preview.start_handle_x = resize_x;
                                        resize_preview.pending_width = *column_width;
                                        resize_preview.preview_x = resize_x;
                                    }
                                }

                                if resize_preview.active && resize_preview.column == Some(i) {
                                    if let Some(pos) = pointer_pos {
                                        let delta = pos.x - resize_preview.start_pointer_x;
                                        let mut new_width = resize_preview.start_width + delta;
                                        if !column.clip {
                                            new_width = new_width.at_least(max_used_widths_ref[i]);
                                        }
                                        resize_preview.pending_width = width_range.clamp(new_width);
                                        resize_preview.preview_x =
                                            resize_preview.start_handle_x + delta;
                                    }

                                    if resize_response.drag_stopped() {
                                        *column_width = resize_preview.pending_width;
                                        resize_preview = ResizePreviewState::default();
                                    }
                                }
                            }

                            let dragging_something_else =
                                ui.input(|i| i.pointer.any_down() || i.pointer.any_pressed());
                            let resize_hover =
                                resize_response.hovered() && !dragging_something_else;
                            let drag_active = if resize_mode == ColumnResizeMode::Live {
                                resize_response.dragged()
                            } else {
                                resize_preview.active && resize_preview.column == Some(i)
                            };

                            if drag_active {
                                let preview_x = if resize_mode == ColumnResizeMode::Deferred {
                                    resize_preview.preview_x
                                } else {
                                    p0.x
                                };
                                resize_info.active = true;
                                resize_info.column = Some(i);
                                resize_info.preview_x = Some(preview_x);
                                resize_info.pending_width = Some(if resize_mode
                                    == ColumnResizeMode::Deferred
                                {
                                    resize_preview.pending_width
                                } else {
                                    *column_width
                                });
                            }

                            if resize_hover || drag_active {
                                ui.ctx().set_cursor_icon(egui::CursorIcon::ResizeColumn);
                            }

                            if resize_mode == ColumnResizeMode::Deferred && drag_active {
                                let preview_x = resize_preview.preview_x;
                                p0 = egui::pos2(preview_x, top);
                                p1 = egui::pos2(preview_x, bottom);
                            }

                            let stroke = if drag_active {
                                ui.style().visuals.widgets.active.bg_stroke
                            } else if resize_hover {
                                ui.style().visuals.widgets.hovered.bg_stroke
                            } else {
                                ui.visuals().widgets.noninteractive.bg_stroke
                            };

                            ui.painter()
                                .with_clip_rect(clip_rect)
                                .line_segment([p0, p1], stroke);
                        }
                    } else {
                        let clip_rect = if is_fixed {
                            ui.clip_rect()
                        } else {
                            scrollable_clip_rect
                        };

                        if header_resize_handled {
                            let stroke = ui.visuals().widgets.noninteractive.bg_stroke;
                            ui.painter()
                                .with_clip_rect(clip_rect)
                                .line_segment([p0, p1], stroke);

                            if resize_mode == ColumnResizeMode::Deferred
                                && resize_info.active
                                && resize_info.column == Some(i)
                            {
                                let preview_x =
                                    resize_info.preview_x.unwrap_or(resize_x);
                                let p0_preview = egui::pos2(preview_x, top);
                                let p1_preview = egui::pos2(preview_x, bottom);
                                let stroke =
                                    ui.style().visuals.widgets.active.bg_stroke;
                                ui.painter()
                                    .with_clip_rect(clip_rect)
                                    .line_segment([p0_preview, p1_preview], stroke);
                            }
                        } else if let Some(header_bottom) = header_bottom {
                            // For header-only resize, the interact_rect is in parent/screen coordinates
                            // but we're inside the scroll area Ui. Use the background layer to
                            // register the interaction in absolute coordinates.
                            let p0_header = egui::pos2(resize_x, table_top);
                            let p1_header = egui::pos2(resize_x, header_bottom);

                            let interact_rect = egui::Rect::from_min_max(p0_header, p1_header)
                                .expand(ui.style().interaction.resize_grab_radius_side);

                            if interact_rect.is_positive() {
                                // Check if pointer is in the header resize rect (in screen coordinates)
                                let pointer_pos = ui.ctx().input(|i| i.pointer.hover_pos());
                                let pointer_in_rect =
                                    pointer_pos.map_or(false, |pos| interact_rect.contains(pos));

                                // Track drag state per column in frame-persistent data
                                let drag_key = column_resize_id.with("header_drag");
                                let was_dragging: bool =
                                    ui.data(|d| d.get_temp(drag_key).unwrap_or(false));

                                let primary_down = ui.ctx().input(|i| i.pointer.primary_down());
                                let primary_pressed =
                                    ui.ctx().input(|i| i.pointer.primary_pressed());

                                // Start drag on press in rect
                                let is_dragging = if primary_pressed && pointer_in_rect {
                                    true
                                } else if was_dragging && primary_down {
                                    true
                                } else {
                                    false
                                };

                                ui.data_mut(|d| d.insert_temp(drag_key, is_dragging));

                                let drag_active = if resize_mode == ColumnResizeMode::Live {
                                    is_dragging
                                } else {
                                    resize_preview.active && resize_preview.column == Some(i)
                                };

                                // Handle drag
                                if resize_mode == ColumnResizeMode::Live {
                                    if is_dragging {
                                        let drag_delta = ui.ctx().input(|i| i.pointer.delta());
                                        if column.auto_size_this_frame {
                                            *column_width =
                                                width_range.clamp(max_used_widths_ref[i]);
                                        } else {
                                            let mut new_width = *column_width + drag_delta.x;
                                            if !column.clip {
                                                new_width = new_width
                                                    .at_least(max_used_widths_ref[i] - 8.0);
                                            }
                                            *column_width = width_range.clamp(new_width);
                                        }
                                        ui.ctx().set_cursor_icon(egui::CursorIcon::ResizeColumn);
                                    } else if pointer_in_rect {
                                        let dragging_something_else =
                                            ui.input(|i| i.pointer.any_down());
                                        if !dragging_something_else {
                                            ui.ctx().set_cursor_icon(egui::CursorIcon::ResizeColumn);
                                        }
                                    }
                                } else {
                                    if primary_pressed && pointer_in_rect {
                                        if let Some(pos) = pointer_pos {
                                            resize_preview.active = true;
                                            resize_preview.column = Some(i);
                                            resize_preview.start_width = *column_width;
                                            resize_preview.start_pointer_x = pos.x;
                                            resize_preview.start_handle_x = resize_x;
                                            resize_preview.pending_width = *column_width;
                                            resize_preview.preview_x = resize_x;
                                        }
                                    }

                                    if is_dragging
                                        && resize_preview.active
                                        && resize_preview.column == Some(i)
                                    {
                                        if let Some(pos) = pointer_pos {
                                            let delta = pos.x - resize_preview.start_pointer_x;
                                            let mut new_width =
                                                resize_preview.start_width + delta;
                                            if !column.clip {
                                                new_width =
                                                    new_width.at_least(max_used_widths_ref[i]);
                                            }
                                            resize_preview.pending_width =
                                                width_range.clamp(new_width);
                                            resize_preview.preview_x =
                                                resize_preview.start_handle_x + delta;
                                        }
                                    }

                                    if was_dragging
                                        && !primary_down
                                        && resize_preview.column == Some(i)
                                    {
                                        *column_width = resize_preview.pending_width;
                                        resize_preview = ResizePreviewState::default();
                                    }

                                    if pointer_in_rect && !primary_down {
                                        let dragging_something_else =
                                            ui.input(|i| i.pointer.any_down());
                                        if !dragging_something_else {
                                            ui.ctx().set_cursor_icon(egui::CursorIcon::ResizeColumn);
                                        }
                                    }

                                    if drag_active {
                                        ui.ctx().set_cursor_icon(egui::CursorIcon::ResizeColumn);
                                    }
                                }

                                if drag_active {
                                    let preview_x = if resize_mode
                                        == ColumnResizeMode::Deferred
                                    {
                                        resize_preview.preview_x
                                    } else {
                                        resize_x
                                    };
                                    resize_info.active = true;
                                    resize_info.column = Some(i);
                                    resize_info.preview_x = Some(preview_x);
                                    resize_info.pending_width = Some(if resize_mode
                                        == ColumnResizeMode::Deferred
                                    {
                                        resize_preview.pending_width
                                    } else {
                                        *column_width
                                    });
                                }

                                if resize_mode == ColumnResizeMode::Deferred && drag_active {
                                    let preview_x = resize_preview.preview_x;
                                    let p0_preview = egui::pos2(preview_x, top);
                                    let p1_preview = egui::pos2(preview_x, bottom);
                                    let stroke =
                                        ui.style().visuals.widgets.active.bg_stroke;
                                    ui.painter()
                                        .with_clip_rect(clip_rect)
                                        .line_segment([p0_preview, p1_preview], stroke);
                                }
                            }
                        }

                        // let stroke = ui.visuals().widgets.noninteractive.bg_stroke;
                        // ui.painter().with_clip_rect(clip_rect).line_segment([p0, p1], stroke);
                    }
                }

                available_width -= *column_width + spacing_x;
            }

            ui.data_mut(|d| {
                d.insert_temp(preview_id, resize_preview);
                d.insert_temp(resize_info_id, resize_info);
            });

            state
        });

        let mut state = scroll_area_out.inner;
        state.scroll_offset = scroll_area_out.state.offset;
        state.store(ui, state_id);

        ScrollAreaOutput {
            state: scroll_area_out.state,
            content_size: scroll_area_out.content_size,
            inner_rect: scroll_area_out.inner_rect,
            id: scroll_area_out.id,
            inner: (),
        }
    }
}

/// The body of a table.
///
/// Is created by calling `body` on a [`Table`] (after adding a header row) or [`TableBuilder`] (without a header row).
pub struct TableBody<'a> {
    layout: StripLayout<'a>,

    columns: &'a [Column],

    /// Current column widths.
    widths: &'a [f32],

    /// Accumulated maximum used widths for each column.
    max_used_widths: &'a mut [f32],

    /// Total width of all fixed columns (for clipping scrollable content)
    fixed_columns_width: f32,

    striped: bool,
    row_index: usize,
    y_range: Rangef,

    /// Look for this row to scroll to.
    scroll_to_row: Option<usize>,

    /// If we find the correct row to scroll to,
    /// this is set to the y-range of the row.
    scroll_to_y_range: &'a mut Option<Rangef>,

    hovered_row_index: Option<usize>,

    /// Used to store the hovered row index between frames.
    hovered_row_index_id: egui::Id,
    state_id: egui::Id,

    /// Custom styling options.
    style: TableStyle,
}

impl<'a> TableBody<'a> {
    /// Access the contained [`egui::Ui`].
    ///
    /// You can use this to e.g. modify the [`egui::Style`] with [`egui::Ui::style_mut`].
    pub fn ui_mut(&mut self) -> &mut egui::Ui {
        self.layout.ui
    }

    /// Where in screen-space is the table body?
    pub fn max_rect(&self) -> Rect {
        self.layout
            .rect
            .translate(egui::vec2(0.0, self.scroll_offset_y()))
    }

    fn scroll_offset_y(&self) -> f32 {
        self.y_range.min - self.layout.rect.top()
    }

    /// Return a vector containing all column widths for this table body.
    ///
    /// This is primarily meant for use with [`TableBody::heterogeneous_rows`] in cases where row
    /// heights are expected to according to the width of one or more cells -- for example, if text
    /// is wrapped rather than clipped within the cell.
    pub fn widths(&self) -> &[f32] {
        self.widths
    }

    /// Add a single row with the given height.
    ///
    /// ⚠️ It is much more performant to use [`Self::rows`] or [`Self::heterogeneous_rows`],
    /// as those functions will only render the visible rows.
    pub fn row(&mut self, height: f32, add_row_content: impl FnOnce(TableRow<'a, '_>)) {
        let mut response: Option<Response> = None;
        let top_y = self.layout.cursor.y;
        let scroll_offset_x = self.layout.ui.ctx().data(|d| {
            d.get_temp::<TableState>(self.state_id)
                .map(|s| s.scroll_offset.x)
                .unwrap_or(0.0)
        });

        add_row_content(TableRow {
            layout: &mut self.layout,
            columns: self.columns,
            widths: self.widths,
            max_used_widths: self.max_used_widths,
            row_index: self.row_index,
            col_index: 0,
            height,
            striped: self.striped && self.row_index.is_multiple_of(2),
            hovered: self.hovered_row_index == Some(self.row_index),
            selected: false,
            overline: false,
            response: &mut response,
            scroll_offset_x,
            fixed_columns_width: self.fixed_columns_width,
            style: self.style.clone(),
        });
        self.capture_hover_state(&response, self.row_index);
        let bottom_y = self.layout.cursor.y;

        if Some(self.row_index) == self.scroll_to_row {
            *self.scroll_to_y_range = Some(Rangef::new(top_y, bottom_y));
        }

        self.row_index += 1;
    }

    /// Add many rows with same height.
    ///
    /// Is a lot more performant than adding each individual row as non visible rows must not be rendered.
    ///
    /// If you need many rows with different heights, use [`Self::heterogeneous_rows`] instead.
    ///
    /// ### Example
    /// ```
    /// # egui::__run_test_ui(|ui| {
    /// use egui_extras::{TableBuilder, Column};
    /// TableBuilder::new(ui)
    ///     .column(Column::remainder().at_least(100.0))
    ///     .body(|mut body| {
    ///         let row_height = 18.0;
    ///         let num_rows = 10_000;
    ///         body.rows(row_height, num_rows, |mut row| {
    ///             let row_index = row.index();
    ///             row.col(|ui| {
    ///                 ui.label(format!("First column of row {row_index}"));
    ///             });
    ///         });
    ///     });
    /// # });
    /// ```
    pub fn rows(
        mut self,
        row_height_sans_spacing: f32,
        total_rows: usize,
        mut add_row_content: impl FnMut(TableRow<'_, '_>),
    ) {
        let spacing = self.layout.ui.spacing().item_spacing;
        let row_height_with_spacing = row_height_sans_spacing + spacing.y;

        let ctx = self.layout.ui.ctx().clone();
        let scroll_offset_x = ctx.data(|d| {
            d.get_temp::<TableState>(self.state_id)
                .map(|s| s.scroll_offset.x)
                .unwrap_or(0.0)
        });

        if let Some(scroll_to_row) = self.scroll_to_row {
            let scroll_to_row = scroll_to_row.at_most(total_rows.saturating_sub(1)) as f32;
            *self.scroll_to_y_range = Some(Rangef::new(
                self.layout.cursor.y + scroll_to_row * row_height_with_spacing,
                self.layout.cursor.y + (scroll_to_row + 1.0) * row_height_with_spacing,
            ));
        }

        let scroll_offset_y = self
            .scroll_offset_y()
            .min(total_rows as f32 * row_height_with_spacing);
        let max_height = self.y_range.span();
        let mut min_row = 0;

        if scroll_offset_y > 0.0 {
            min_row = (scroll_offset_y / row_height_with_spacing).floor() as usize;
            self.add_buffer(min_row as f32 * row_height_with_spacing);
        }

        let max_row =
            ((scroll_offset_y + max_height) / row_height_with_spacing).ceil() as usize + 1;
        let max_row = max_row.min(total_rows);

        for row_index in min_row..max_row {
            let mut response: Option<Response> = None;
            add_row_content(TableRow {
                layout: &mut self.layout,
                columns: self.columns,
                widths: self.widths,
                max_used_widths: self.max_used_widths,
                row_index,
                col_index: 0,
                height: row_height_sans_spacing,
                striped: self.striped && (row_index + self.row_index).is_multiple_of(2),
                hovered: self.hovered_row_index == Some(row_index),
                selected: false,
                overline: false,
                response: &mut response,
                scroll_offset_x,
                fixed_columns_width: self.fixed_columns_width,
                style: self.style.clone(),
            });
            self.capture_hover_state(&response, row_index);
        }

        if total_rows - max_row > 0 {
            let skip_height = (total_rows - max_row) as f32 * row_height_with_spacing;
            self.add_buffer(skip_height - spacing.y);
        }
    }

    /// Add rows with varying heights.
    ///
    /// This takes a very slight performance hit compared to [`TableBody::rows`] due to the need to
    /// iterate over all row heights in order to calculate the virtual table height above and below the
    /// visible region, but it is many orders of magnitude more performant than adding individual
    /// heterogeneously-sized rows using [`TableBody::row`] at the cost of the additional complexity
    /// that comes with pre-calculating row heights and representing them as an iterator.
    ///
    /// ### Example
    /// ```
    /// # egui::__run_test_ui(|ui| {
    /// use egui_extras::{TableBuilder, Column};
    /// TableBuilder::new(ui)
    ///     .column(Column::remainder().at_least(100.0))
    ///     .body(|mut body| {
    ///         let row_heights: Vec<f32> = vec![60.0, 18.0, 31.0, 240.0];
    ///         body.heterogeneous_rows(row_heights.into_iter(), |mut row| {
    ///             let row_index = row.index();
    ///             let thick = row_index % 6 == 0;
    ///             row.col(|ui| {
    ///                 ui.centered_and_justified(|ui| {
    ///                     ui.label(row_index.to_string());
    ///                 });
    ///             });
    ///         });
    ///     });
    /// # });
    /// ```
    pub fn heterogeneous_rows(
        mut self,
        heights: impl Iterator<Item = f32>,
        mut add_row_content: impl FnMut(TableRow<'_, '_>),
    ) {
        let spacing = self.layout.ui.spacing().item_spacing;
        let mut enumerated_heights = heights.enumerate();

        let max_height = self.y_range.span();
        let scroll_offset_y = self.scroll_offset_y() as f64;

        let scroll_to_y_range_offset = self.layout.cursor.y as f64;

        let mut cursor_y: f64 = 0.0;

        let scroll_offset_x = self.layout.ui.ctx().data(|d| {
            d.get_temp::<TableState>(self.state_id)
                .map(|s| s.scroll_offset.x)
                .unwrap_or(0.0)
        });

        // Skip the invisible rows, and populate the first non-virtual row.
        for (row_index, row_height) in &mut enumerated_heights {
            let old_cursor_y = cursor_y;
            cursor_y += (row_height + spacing.y) as f64;

            if Some(row_index) == self.scroll_to_row {
                *self.scroll_to_y_range = Some(Rangef::new(
                    (scroll_to_y_range_offset + old_cursor_y) as f32,
                    (scroll_to_y_range_offset + cursor_y) as f32,
                ));
            }

            if cursor_y >= scroll_offset_y {
                // This row is visible:
                self.add_buffer(old_cursor_y as f32); // skip all the invisible rows
                let mut response: Option<Response> = None;
                add_row_content(TableRow {
                    layout: &mut self.layout,
                    columns: self.columns,
                    widths: self.widths,
                    max_used_widths: self.max_used_widths,
                    row_index,
                    col_index: 0,
                    height: row_height,
                    striped: self.striped && (row_index + self.row_index).is_multiple_of(2),
                    hovered: self.hovered_row_index == Some(row_index),
                    selected: false,
                    overline: false,
                    response: &mut response,
                    scroll_offset_x,
                    fixed_columns_width: self.fixed_columns_width,
                    style: self.style.clone(),
                });
                self.capture_hover_state(&response, row_index);
                break;
            }
        }

        // populate visible rows:
        for (row_index, row_height) in &mut enumerated_heights {
            let top_y = cursor_y;
            let mut response: Option<Response> = None;
            add_row_content(TableRow {
                layout: &mut self.layout,
                columns: self.columns,
                widths: self.widths,
                max_used_widths: self.max_used_widths,
                row_index,
                col_index: 0,
                height: row_height,
                striped: self.striped && (row_index + self.row_index).is_multiple_of(2),
                hovered: self.hovered_row_index == Some(row_index),
                overline: false,
                selected: false,
                response: &mut response,
                scroll_offset_x,
                fixed_columns_width: self.fixed_columns_width,
                style: self.style.clone(),
            });
            self.capture_hover_state(&response, row_index);
            cursor_y += (row_height + spacing.y) as f64;

            if Some(row_index) == self.scroll_to_row {
                *self.scroll_to_y_range = Some(Rangef::new(
                    (scroll_to_y_range_offset + top_y) as f32,
                    (scroll_to_y_range_offset + cursor_y) as f32,
                ));
            }

            if cursor_y > scroll_offset_y + max_height as f64 {
                break;
            }
        }

        // calculate height below the visible table range:
        let mut height_below_visible: f64 = 0.0;
        for (row_index, row_height) in enumerated_heights {
            height_below_visible += (row_height + spacing.y) as f64;

            let top_y = cursor_y;
            cursor_y += (row_height + spacing.y) as f64;
            if Some(row_index) == self.scroll_to_row {
                *self.scroll_to_y_range = Some(Rangef::new(
                    (scroll_to_y_range_offset + top_y) as f32,
                    (scroll_to_y_range_offset + cursor_y) as f32,
                ));
            }
        }

        if self.scroll_to_row.is_some() && self.scroll_to_y_range.is_none() {
            // Catch desire to scroll past the end:
            *self.scroll_to_y_range =
                Some(Rangef::point((scroll_to_y_range_offset + cursor_y) as f32));
        }

        if height_below_visible > 0.0 {
            // we need to add a buffer to allow the table to
            // accurately calculate the scrollbar position
            self.add_buffer(height_below_visible as f32);
        }
    }

    // Create a table row buffer of the given height to represent the non-visible portion of the
    // table.
    fn add_buffer(&mut self, height: f32) {
        self.layout.skip_space(egui::vec2(0.0, height));
    }

    // Capture the hover information for the just created row. This is used in the next render
    // to ensure that the entire row is highlighted.
    fn capture_hover_state(&self, response: &Option<Response>, row_index: usize) {
        let is_row_hovered = response.as_ref().is_some_and(|r| r.hovered());
        if is_row_hovered {
            self.layout
                .ui
                .data_mut(|data| data.insert_temp(self.hovered_row_index_id, row_index));
        }
    }
}

impl Drop for TableBody<'_> {
    fn drop(&mut self) {
        self.layout.allocate_rect();
    }
}

/// The row of a table.
/// Is created by [`TableRow`] for each created [`TableBody::row`] or each visible row in rows created by calling [`TableBody::rows`].
pub struct TableRow<'a, 'b> {
    layout: &'b mut StripLayout<'a>,
    columns: &'b [Column],
    widths: &'b [f32],

    /// grows during building with the maximum widths
    max_used_widths: &'b mut [f32],

    scroll_offset_x: f32,

    /// Total width of fixed columns (for clipping scrollable content)
    fixed_columns_width: f32,

    row_index: usize,
    col_index: usize,
    height: f32,

    striped: bool,
    hovered: bool,
    selected: bool,
    overline: bool,

    response: &'b mut Option<Response>,

    /// Custom styling options.
    style: TableStyle,
}

impl TableRow<'_, '_> {
    /// Add the contents of a column on this row (i.e. a cell).
    /// Returns the used space (`min_rect`) plus the [`Response`] of the whole cell.
    #[cfg_attr(debug_assertions, track_caller)]
    pub fn col(&mut self, add_cell_contents: impl FnOnce(&mut Ui)) -> (Rect, Response) {
        let col_index = self.col_index;
        let column = self.columns.get(col_index);
        let clip = column.is_some_and(|c| c.clip);
        let auto_size_this_frame = column.is_some_and(|c| c.auto_size_this_frame);
        let is_fixed = column.is_some_and(|c| c.fixed);

        let width_f32 = if let Some(width) = self.widths.get(col_index) {
            self.col_index += 1;
            *width
        } else {
            crate::log_or_panic!(
                "Added more `Table` columns than were pre-allocated ({} pre-allocated)",
                self.columns.len()
            );
            return (
                Rect::NOTHING,
                self.layout
                    .ui
                    .allocate_response(egui::Vec2::ZERO, egui::Sense::hover()),
            );
        };

        let width = crate::layout::CellSize::Absolute(width_f32);
        let height = crate::layout::CellSize::Absolute(self.height);
        let flags = crate::layout::StripLayoutFlags {
            clip,
            striped: self.striped,
            hovered: self.hovered,
            selected: self.selected,
            overline: self.overline,
            sizing_pass: auto_size_this_frame,
            is_fixed,
            fixed_columns_width: self.fixed_columns_width,
            scroll_offset_x: self.scroll_offset_x,
            // Style overrides
            selection_bg_color: self.style.selection_bg_color,
            striped_bg_color: self.style.striped_bg_color,
            hovered_bg_color: self.style.hovered_bg_color,
            vertical_grid_stroke: self.style.vertical_grid_stroke,
            horizontal_grid_stroke: self.style.horizontal_grid_stroke,
            selection_stroke: self.style.selection_stroke,
        };

        let scroll_offset_x = self.scroll_offset_x;
        let fixed_columns_width = self.fixed_columns_width;

        let (used_rect, response) = self.layout.add(
            flags,
            width,
            height,
            egui::Id::new(self.row_index).with(col_index),
            add_cell_contents,
        );

        if let Some(max_w) = self.max_used_widths.get_mut(col_index) {
            *max_w = max_w.max(used_rect.width());
        }

        if let Some(r) = self.response {
            *r = r.union(response.clone());
        } else {
            *self.response = Some(response.clone());
        }

        (used_rect, response)
    }

    /// Set the selection highlight state for cells added after a call to this function.
    #[inline]
    pub fn set_selected(&mut self, selected: bool) {
        self.selected = selected;
    }

    /// Set the hovered highlight state for cells added after a call to this function.
    #[inline]
    pub fn set_hovered(&mut self, hovered: bool) {
        self.hovered = hovered;
    }

    /// Set the overline state for this row. The overline is a line above the row,
    /// usable for e.g. visually grouping rows.
    #[inline]
    pub fn set_overline(&mut self, overline: bool) {
        self.overline = overline;
    }

    /// Returns a union of the [`Response`]s of the cells added to the row up to this point.
    ///
    /// You need to add at least one row to the table before calling this function.
    pub fn response(&self) -> Response {
        self.response
            .clone()
            .expect("Should only be called after `col`")
    }

    /// Returns the index of the row.
    #[inline]
    pub fn index(&self) -> usize {
        self.row_index
    }

    /// Returns the index of the column. Incremented after a column is added.
    #[inline]
    pub fn col_index(&self) -> usize {
        self.col_index
    }
}

impl Drop for TableRow<'_, '_> {
    #[inline]
    fn drop(&mut self) {
        self.layout.end_line();
    }
}
