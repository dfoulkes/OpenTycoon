//! Streets: where walkers may stand and the shortest way between two cells (§4.1, R22, R25).
//!
//! The walking grid ([`super::movement`]) has a one-cell road ring around every lot. A
//! **street cell** is a road cell that is not water. The river (`map.yaml` `river`) runs along
//! the road row south of `south_of_row`, so that whole road row is water except its bridges.
//! The bridge for column `c` carries the north–south road on the west side of that column
//! (cells `x = c × pitch_x`) over the river. Walkers stand only on street cells: lots are
//! entered only at a destination, and a destination is the street cell at its frontage
//! ([`Streets::frontage`]), so nobody ever cuts through a lot or a building.
//!
//! Junctions are the road crossings, cell `(i × pitch_x, j × pitch_y)`. Between two junctions
//! a road is a straight corridor, so a shortest route between two street cells is either
//! straight along one road (when nothing is in the way) or out to one end of the start's
//! segment, junction to junction, and in from one end of the end's segment. The junction to
//! junction distances and next hops are computed once per board (Floyd–Warshall over the
//! `(W + 1) × (H + 1)` junctions, 100 on the full board), so a route length costs at most four
//! table reads and a step a handful more. Integers only; equal routes are broken the same way
//! every time (north, west, east, south; the near end of a segment before the far end).

use super::movement::{Cell, Geometry, step_towards};
use crate::config::Config;
use crate::land::{Block, Footprint, Map};

/// Unreachable, and still safe to add twice.
const FAR: u32 = u32::MAX / 4;
/// No next hop.
const NONE: u8 = u8::MAX;
/// Steps between junctions in tie-break order: north, west, east, south.
const DIRS: [(i32, i32); 4] = [(0, -1), (-1, 0), (1, 0), (0, 1)];

/// The street network of one board: which cells are street, and the shortest routes between
/// junctions. Built once per board ([`Streets::new`]); read-only afterwards.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Streets {
    geometry: Geometry,
    /// Junction columns (board width + 1).
    cols: i32,
    /// Junction rows (board height + 1).
    rows: i32,
    /// The road row the river takes, if the board has one.
    river_y: Option<i32>,
    /// Bridge cells' x on the river row, sorted.
    bridges_x: Vec<i32>,
    /// Shortest street distance in cells between junctions, `cols × rows` squared.
    dist: Vec<u32>,
    /// First step (index into [`DIRS`]) on a shortest route between junctions.
    next: Vec<u8>,
    /// Junctions: `cols × rows`.
    count: usize,
}

impl Streets {
    /// The streets of `map` with lots of `geometry`'s size.
    pub fn new(map: &Map, geometry: Geometry) -> Self {
        let (px, py) = (geometry.pitch_x(), geometry.pitch_y());
        let cols = i32::try_from(map.width).unwrap_or(0) + 1;
        let rows = i32::try_from(map.height).unwrap_or(0) + 1;
        let river_y = map.river.as_ref().map(|r| {
            let below = i32::try_from(r.south_of_row).unwrap_or(i32::MAX - 1) + 1;
            below.saturating_mul(py)
        });
        let bridges_x = map
            .river
            .as_ref()
            .map(|r| {
                r.bridges
                    .iter()
                    .map(|c| i32::try_from(*c).unwrap_or(0).saturating_mul(px))
                    .collect()
            })
            .unwrap_or_default();
        let mut streets = Streets {
            geometry,
            cols,
            rows,
            river_y,
            bridges_x,
            dist: Vec::new(),
            next: Vec::new(),
            count: usize::try_from(cols.max(0) * rows.max(0)).unwrap_or(0),
        };
        streets.route_junctions();
        streets
    }

    /// The streets of `map` with the configured lot size (`game.yaml` `world.lot_size`).
    pub fn for_config(map: &Map, config: &Config) -> Self {
        let [lot_w, lot_d] = config.game.world.lot_size;
        Self::new(map, Geometry { lot_w, lot_d })
    }

    /// The walking grid's shape.
    pub fn geometry(&self) -> Geometry {
        self.geometry
    }

    /// True if `c` is a road cell (the ring around a lot), whether or not it is water.
    pub fn is_road(&self, c: Cell) -> bool {
        let (px, py) = (self.geometry.pitch_x(), self.geometry.pitch_y());
        let in_bounds =
            c.x >= 0 && c.y >= 0 && c.x <= (self.cols - 1) * px && c.y <= (self.rows - 1) * py;
        in_bounds && (c.x % px == 0 || c.y % py == 0)
    }

    /// True if `c` is on the river: the river's road row, away from a bridge.
    pub fn is_water(&self, c: Cell) -> bool {
        self.river_y == Some(c.y) && self.is_road(c) && !self.bridges_x.contains(&c.x)
    }

    /// True if `c` is a bridge over the river.
    pub fn is_bridge(&self, c: Cell) -> bool {
        self.river_y == Some(c.y) && self.bridges_x.contains(&c.x) && self.is_road(c)
    }

    /// True if a walker may stand on `c`: a road cell that is not water.
    pub fn is_street(&self, c: Cell) -> bool {
        self.is_road(c) && !self.is_water(c)
    }

    /// A building's frontage: the street cell outside the middle of the first lot edge it
    /// touches (north, west, east, south) that is not water. A building that touches only a
    /// riverside edge uses the nearest side street instead. Walkers come and go here.
    pub fn frontage(&self, block: &Block, footprint: Footprint) -> Cell {
        let g = self.geometry;
        let o = g.origin(block);
        let int = |v: u32| i32::try_from(v).unwrap_or(0);
        let (lw, ld) = (int(g.lot_w), int(g.lot_d));
        let (fx, fy) = (int(footprint.x), int(footprint.y));
        let (fw, fd) = (int(footprint.w), int(footprint.d));
        let mid_x = o.x + (fx + fw / 2).min(lw - 1).max(0);
        let mid_y = o.y + (fy + fd / 2).min(ld - 1).max(0);
        let sides = [
            (
                fy == 0,
                Cell {
                    x: mid_x,
                    y: o.y - 1,
                },
            ),
            (
                fx == 0,
                Cell {
                    x: o.x - 1,
                    y: mid_y,
                },
            ),
            (
                fx + fw >= lw,
                Cell {
                    x: o.x + lw,
                    y: mid_y,
                },
            ),
            (
                fy + fd >= ld,
                Cell {
                    x: mid_x,
                    y: o.y + ld,
                },
            ),
        ];
        if let Some((_, c)) = sides
            .iter()
            .find(|(touches, c)| *touches && self.is_street(*c))
        {
            return *c;
        }
        // Only a riverside edge (or none, which validation forbids): the nearest side street.
        // The west and east sides are never water, so there is always one.
        let door = Cell { x: mid_x, y: mid_y };
        sides
            .iter()
            .map(|(_, c)| *c)
            .filter(|c| self.is_street(*c))
            .min_by_key(|c| door.manhattan(*c))
            .unwrap_or(sides[1].1)
    }

    /// Where a block's tube station meets the street (R25): the frontage of the whole lot.
    pub fn entrance(&self, block: &Block) -> Cell {
        let whole = Footprint {
            x: 0,
            y: 0,
            w: self.geometry.lot_w,
            d: self.geometry.lot_d,
        };
        self.frontage(block, whole)
    }

    /// The length in cells of a shortest street route from `a` to `b`. Cells off the streets
    /// fall back to the Manhattan distance; `b` unreachable from `a` is very far.
    pub fn route_len(&self, a: Cell, b: Cell) -> u32 {
        if !self.is_street(a) || !self.is_street(b) || self.straight(a, b) {
            return a.manhattan(b);
        }
        self.via(a, b).map_or(FAR, |(cells, _, _)| cells)
    }

    /// Street distance in blocks, for `distance_decay` (§8.3): route cells over the block
    /// pitch.
    #[allow(
        clippy::float_arithmetic,
        reason = "distance in blocks feeds customer scoring, the one float module"
    )]
    pub fn blocks_between(&self, a: Cell, b: Cell) -> f64 {
        f64::from(self.route_len(a, b)) / f64::from(self.geometry.lot_w + 1)
    }

    /// Moves `from` up to `cells` cells along a shortest street route towards `to`. Returns the
    /// new cell. Off the streets it falls back to the plain grid walk.
    pub fn step(&self, from: Cell, to: Cell, cells: u32) -> Cell {
        if !self.is_street(from) || !self.is_street(to) {
            return step_towards(from, to, cells);
        }
        let (px, py) = (self.geometry.pitch_x(), self.geometry.pitch_y());
        let mut at = from;
        let mut left = cells;
        while left > 0 && at != to {
            // Each leg is a straight line along one road: to `to` if nothing is in the way,
            // else to the near end of this segment, else on to the next junction.
            let target = if self.straight(at, to) {
                to
            } else {
                let Some((_, start, end)) = self.via(at, to) else {
                    break;
                };
                let corner = self.junction_cell(start);
                if at == corner {
                    let hop = self
                        .next
                        .get(start * self.count + end)
                        .copied()
                        .unwrap_or(NONE);
                    let Some((dx, dy)) = DIRS.get(usize::from(hop)) else {
                        break;
                    };
                    Cell {
                        x: at.x + dx * px,
                        y: at.y + dy * py,
                    }
                } else {
                    corner
                }
            };
            let leg = at.manhattan(target).min(left);
            if leg == 0 {
                break;
            }
            at = step_towards(at, target, leg);
            left -= leg;
        }
        at
    }

    /// True if `a` to `b` is a straight walk along one road with no water in the way: then it
    /// is a shortest route (it is the Manhattan distance). Both must be street cells.
    fn straight(&self, a: Cell, b: Cell) -> bool {
        let (px, py) = (self.geometry.pitch_x(), self.geometry.pitch_y());
        if a == b {
            return true;
        }
        if a.y == b.y && a.y.rem_euclid(py) == 0 {
            return self.river_y != Some(a.y);
        }
        if a.x == b.x && a.x.rem_euclid(px) == 0 {
            let (lo, hi) = (a.y.min(b.y), a.y.max(b.y));
            return self
                .river_y
                .is_none_or(|r| !(lo..=hi).contains(&r) || self.bridges_x.contains(&a.x));
        }
        false
    }

    /// The shortest way from `a` to `b` through junctions: its length, the junction it leaves
    /// `a`'s segment by and the one it joins `b`'s segment at. Ties go to the first found (the
    /// north or west end before the south or east end). `None` if `b` is unreachable.
    fn via(&self, a: Cell, b: Cell) -> Option<(u32, usize, usize)> {
        let mut best: Option<(u32, usize, usize)> = None;
        let ends = self.anchors(b);
        for (start, out) in self.anchors(a).into_iter().flatten() {
            for (end, into) in ends.into_iter().flatten() {
                let between = self
                    .dist
                    .get(start * self.count + end)
                    .copied()
                    .unwrap_or(FAR);
                if between >= FAR {
                    continue;
                }
                let cells = out + between + into;
                if best.is_none_or(|(known, _, _)| cells < known) {
                    best = Some((cells, start, end));
                }
            }
        }
        best
    }

    /// The junctions at the two ends of `c`'s road segment and the cells to each (just itself
    /// if `c` is a junction). A junction on the river away from a bridge is no end.
    fn anchors(&self, c: Cell) -> [Option<(usize, u32)>; 2] {
        let (px, py) = (self.geometry.pitch_x(), self.geometry.pitch_y());
        let (i, xm) = (c.x.div_euclid(px), c.x.rem_euclid(px));
        let (j, ym) = (c.y.div_euclid(py), c.y.rem_euclid(py));
        let end =
            |i: i32, j: i32, cells: i32| self.junction(i, j).map(|n| (n, cells.unsigned_abs()));
        match (xm == 0, ym == 0) {
            (true, true) => [end(i, j, 0), None],
            (false, true) => [end(i, j, xm), end(i + 1, j, px - xm)],
            (true, false) => [end(i, j, ym), end(i, j + 1, py - ym)],
            (false, false) => [None, None],
        }
    }

    /// The index of junction `(i, j)`, if it is on the board and not water.
    fn junction(&self, i: i32, j: i32) -> Option<usize> {
        if i < 0 || j < 0 || i >= self.cols || j >= self.rows {
            return None;
        }
        let (px, py) = (self.geometry.pitch_x(), self.geometry.pitch_y());
        if !self.is_street(Cell {
            x: i * px,
            y: j * py,
        }) {
            return None;
        }
        usize::try_from(j * self.cols + i).ok()
    }

    /// Junction `n`'s cell.
    fn junction_cell(&self, n: usize) -> Cell {
        let n = i32::try_from(n).unwrap_or(0);
        let cols = self.cols.max(1);
        Cell {
            x: (n % cols) * self.geometry.pitch_x(),
            y: (n / cols) * self.geometry.pitch_y(),
        }
    }

    /// The junction one segment from `(i, j)` along `(dx, dy)` and the segment's length in
    /// cells, if the whole segment is street. A segment is one road and the river takes whole
    /// road rows, so its first cell says.
    fn segment(&self, i: i32, j: i32, (dx, dy): (i32, i32)) -> Option<(usize, u32)> {
        let to = self.junction(i + dx, j + dy)?;
        let (px, py) = (self.geometry.pitch_x(), self.geometry.pitch_y());
        if !self.is_street(Cell {
            x: i * px + dx,
            y: j * py + dy,
        }) {
            return None;
        }
        let len = if dx == 0 { py } else { px };
        Some((to, len.unsigned_abs()))
    }

    /// Shortest distances and first hops between every pair of junctions (Floyd–Warshall),
    /// once per board.
    fn route_junctions(&mut self) {
        let n = self.count;
        let mut dist = vec![FAR; n * n];
        for j in 0..self.rows {
            for i in 0..self.cols {
                let Some(a) = self.junction(i, j) else {
                    continue;
                };
                dist[a * n + a] = 0;
                for dir in DIRS {
                    if let Some((b, len)) = self.segment(i, j, dir) {
                        dist[a * n + b] = len;
                    }
                }
            }
        }
        for k in 0..n {
            let through: Vec<u32> = dist[k * n..(k + 1) * n].to_vec();
            for a in 0..n {
                let to_k = dist[a * n + k];
                if to_k >= FAR {
                    continue;
                }
                for (ab, kb) in dist[a * n..(a + 1) * n].iter_mut().zip(&through) {
                    let via = to_k + kb;
                    if via < *ab {
                        *ab = via;
                    }
                }
            }
        }
        let mut next = vec![NONE; n * n];
        for j in 0..self.rows {
            for i in 0..self.cols {
                let Some(a) = self.junction(i, j) else {
                    continue;
                };
                for b in 0..n {
                    let want = dist[a * n + b];
                    if a == b || want >= FAR {
                        continue;
                    }
                    let hop = DIRS.iter().position(|dir| {
                        self.segment(i, j, *dir)
                            .is_some_and(|(m, len)| len + dist[m * n + b] == want)
                    });
                    next[a * n + b] = hop.and_then(|h| u8::try_from(h).ok()).unwrap_or(NONE);
                }
            }
        }
        self.dist = dist;
        self.next = next;
    }
}
