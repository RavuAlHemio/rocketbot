use crate::graph_drawing::{Canvas, ChartColor};


#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct LineGraph {
    thicken: usize,
    canvas: Canvas,
}
impl LineGraph {
    pub fn canvas(&self) -> &Canvas { &self.canvas }
    pub fn canvas_mut(&mut self) -> &mut Canvas { &mut self.canvas }

    fn calculate_image_size(x_positions: usize, max_y_value: usize) -> (usize, usize) {
        // 2 = frame width on both edges
        let width = 2 + x_positions;
        let height = 2 + Canvas::data_height_with_headroom(max_y_value);

        // crash early if the dimensions are too large
        u32::try_from(width).expect("width too large");
        u32::try_from(height).expect("height too large");

        (width, height)
    }

    pub fn new_for_ranges(x_positions: usize, max_y_value: usize, thicken: usize) -> Self {
        let (width, height) = Self::calculate_image_size(x_positions, max_y_value);
        let canvas = Canvas::new(width, height);
        let mut image = Self {
            thicken,
            canvas,
        };

        // draw ticks
        const HORIZONTAL_TICK_STEP: usize = 100;
        const VERTICAL_TICK_STEP: usize = 100;
        for graph_y in (0..height).step_by(HORIZONTAL_TICK_STEP) {
            let y = height - (1 + graph_y);
            for x in 1..(width-1) {
                image.canvas.set_pixel(x, y, ChartColor::Tick);
            }
        }
        for graph_x in (0..width).step_by(VERTICAL_TICK_STEP) {
            let x = 1 + graph_x;
            for y in 1..(height-1) {
                image.canvas.set_pixel(x, y, ChartColor::Tick);
            }
        }

        // draw frame
        for y in 0..height {
            image.canvas.set_pixel(0, y, ChartColor::Border);
            image.canvas.set_pixel(width - 1, y, ChartColor::Border);
        }
        for x in 0..width {
            image.canvas.set_pixel(x, 0, ChartColor::Border);
            image.canvas.set_pixel(x, height - 1, ChartColor::Border);
        }

        image
    }

    pub fn draw_data_point(&mut self, graph_x: usize, value: usize, color: u8) {
        let x = 1 + graph_x;
        let y = self.canvas.height() - (1 + value);
        let pixel_value = ChartColor::Data(color);

        self.canvas.set_pixel(x, y, pixel_value);

        for graph_thicker_y in 0..self.thicken {
            let thicker_y_down = y + 1 + graph_thicker_y;
            if thicker_y_down < self.canvas.height() {
                self.canvas.set_pixel(x, thicker_y_down, pixel_value);
            }

            if let Some(thicker_y_up) = y.checked_sub(1 + graph_thicker_y) {
                self.canvas.set_pixel(x, thicker_y_up, pixel_value);
            }
        }
    }

    pub fn draw_time_subdivision(&mut self, graph_x: usize) {
        let x = 1 + graph_x;
        for y in 1..(self.canvas.height()-1) {
            self.canvas.set_pixel(x, y, ChartColor::TimeSubdivision);
        }
    }
}
