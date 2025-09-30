use crate::graph_drawing::{Canvas, ChartColor, GRAPH_COLORS};


const BAR_SPACING_FROM_CHUNK_EDGE: usize = 2;


#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct BarGraph {
    bar_thickness: usize,
    bars_per_chunk: usize,
    chunk_count: usize,
    canvas: Canvas,
}
impl BarGraph {
    pub fn canvas(&self) -> &Canvas { &self.canvas }
    pub fn canvas_mut(&mut self) -> &mut Canvas { &mut self.canvas }

    fn calculate_image_size(bar_thickness: usize, bars_per_chunk: usize, chunk_count: usize, max_y_value: usize) -> (usize, usize) {
        // 2 = frame width on both edges
        let width =
            2 // left frame + right frame
            + (chunk_count - 1) // chunk separators
            + 2*BAR_SPACING_FROM_CHUNK_EDGE*chunk_count // space between chunk separator and outermost bar (left + right)
            + bar_thickness*bars_per_chunk*chunk_count // the bars themselves
        ;
        let height = 2 + Canvas::data_height_with_headroom(max_y_value);

        // crash early if the dimensions are too large
        u32::try_from(width).expect("width too large");
        u32::try_from(height).expect("height too large");

        (width, height)
    }

    pub fn new_for_ranges(bar_thickness: usize, bars_per_chunk: usize, chunk_count: usize, max_y_value: usize) -> Self {
        let (width, height) = Self::calculate_image_size(
            bar_thickness,
            bars_per_chunk,
            chunk_count,
            max_y_value,
        );
        let canvas = Canvas::new(width, height);
        let mut image = Self {
            bar_thickness,
            bars_per_chunk,
            chunk_count,
            canvas,
        };

        // draw horizontal ticks
        const HORIZONTAL_TICK_STEP: usize = 100;
        for graph_y in (0..height).step_by(HORIZONTAL_TICK_STEP) {
            let y = height - (1 + graph_y);
            for x in 1..(width-1) {
                image.canvas.set_pixel(x, y, ChartColor::Tick);
            }
        }

        // draw vertical chunk separators
        for graph_x in (0..width).step_by(2*BAR_SPACING_FROM_CHUNK_EDGE + bar_thickness*bars_per_chunk) {
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

    fn draw_chunk_bar(&mut self, chunk_index: usize, bar_index: usize, value: usize, color: u8) {
        let bar_x =
            chunk_index * (
                1 // chunk-separator or frame
                + 2*BAR_SPACING_FROM_CHUNK_EDGE // left + right space between chunk separator and bars
                + self.bars_per_chunk*self.bar_thickness // bars
            )
            + 1 // chunk-separator or frame
            + BAR_SPACING_FROM_CHUNK_EDGE // left space from chunk separator
            + bar_index*self.bar_thickness // preceding bars
        ;

        for x in bar_x..bar_x+self.bar_thickness {
            for graph_y in 0..value {
                let y = (self.canvas.height() + 1) - graph_y;
                self.canvas.set_pixel_if_in_range(x, y, ChartColor::Data(color));
            }
        }
    }

    pub fn draw_chunk_bars(&mut self, chunk_index: usize, values: &[usize]) {
        assert_eq!(values.len(), self.bars_per_chunk);

        for (bar_index, value) in values.iter().copied().enumerate() {
            let color = u8::try_from(bar_index % GRAPH_COLORS.len()).unwrap();
            self.draw_chunk_bar(chunk_index, bar_index, value, color);
        }
    }

    pub fn draw_time_subdivision(&mut self, before_chunk_index: usize) {
        let x =
            before_chunk_index * (
                1 // chunk-separator or frame
                + 2*BAR_SPACING_FROM_CHUNK_EDGE // left + right space between chunk separator and bars
                + self.bars_per_chunk*self.bar_thickness // bars
            )
        ;
        for y in 1..(self.canvas.height()-1) {
            self.canvas.set_pixel(x, y, ChartColor::TimeSubdivision);
        }
    }
}
