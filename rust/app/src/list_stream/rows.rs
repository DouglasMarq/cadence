use std::ops::Range;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ListRow(pub usize);

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct RowRanges(Vec<Range<usize>>);

impl RowRanges {
    pub const WHOLE_LIST: Range<usize> = 0..usize::MAX;

    pub fn whole_list() -> Self {
        Self(vec![Self::WHOLE_LIST])
    }

    pub fn from_ranges(mut ranges: Vec<Range<usize>>) -> Self {
        ranges.retain(|range| !range.is_empty());
        ranges.sort_by_key(|range| range.start);
        Self(ranges.into_iter().fold(Vec::new(), merge_into))
    }

    pub fn spanning(rows: Range<usize>) -> Self {
        let mut spanned = Self::default();
        spanned.insert(rows);
        spanned
    }

    pub fn covering(rows: &[usize]) -> Vec<Range<usize>> {
        let single_rows = rows.iter().map(|&row| row..row.saturating_add(1)).collect();
        Self::from_ranges(single_rows).0
    }

    pub fn into_ranges(self) -> Vec<Range<usize>> {
        self.0
    }

    pub fn rows(&self) -> impl Iterator<Item = usize> + '_ {
        self.0.iter().flat_map(Clone::clone)
    }

    pub fn first_row(&self) -> Option<ListRow> {
        self.0.first().map(|range| ListRow(range.start))
    }

    pub fn end(&self) -> usize {
        self.0.last().map_or(0, |range| range.end)
    }

    pub fn contains(&self, row: ListRow) -> bool {
        self.0
            .get(self.first_ending_after(row.0))
            .is_some_and(|range| range.contains(&row.0))
    }

    pub fn intersects(&self, rows: &Range<usize>) -> bool {
        self.0
            .get(self.first_ending_after(rows.start))
            .is_some_and(|range| range.start < rows.end)
    }

    pub fn clamped(&self, row_count: usize) -> Self {
        let clamped = self
            .0
            .iter()
            .map(|range| range.start.min(row_count)..range.end.min(row_count))
            .collect();
        Self::from_ranges(clamped)
    }

    pub fn row_count(&self) -> usize {
        self.0
            .iter()
            .fold(0, |count: usize, range| count.saturating_add(range.len()))
    }

    pub fn nth_row(&self, mut offset: usize) -> Option<ListRow> {
        for range in &self.0 {
            match range.start.checked_add(offset) {
                Some(row) if row < range.end => return Some(ListRow(row)),
                _ => offset = offset.saturating_sub(range.len()),
            }
        }
        None
    }

    pub fn insert(&mut self, rows: Range<usize>) {
        if rows.is_empty() {
            return;
        }
        let first = self.0.partition_point(|range| range.end < rows.start);
        let past_last = self.0.partition_point(|range| range.start <= rows.end);
        let touched = first..past_last.max(first);
        let merged = self
            .0
            .get(touched.clone())
            .map_or(rows.clone(), |touching| {
                spanning(touching).map_or(rows.clone(), |span| union(&span, &rows))
            });
        self.0.splice(touched, [merged]);
    }

    pub fn remove(&mut self, row: ListRow) {
        let position = self.first_ending_after(row.0);
        let Some(range) = self.0.get(position).filter(|range| range.contains(&row.0)) else {
            return;
        };
        let pieces = [range.start..row.0, row.0 + 1..range.end];
        let kept = pieces.into_iter().filter(|piece| !piece.is_empty());
        self.0.splice(position..=position, kept.collect::<Vec<_>>());
    }

    pub fn toggle(&mut self, row: ListRow) {
        match self.contains(row) {
            true => self.remove(row),
            false => self.insert(row.0..row.0.saturating_add(1)),
        }
    }

    pub fn symmetric_difference(&self, other: &Self) -> Self {
        let mut boundaries: Vec<usize> = self
            .0
            .iter()
            .chain(&other.0)
            .flat_map(|range| [range.start, range.end])
            .collect();
        boundaries.sort_unstable();
        boundaries.dedup();
        let differing = boundaries
            .windows(2)
            .filter_map(|pair| match pair {
                [start, end] => Some(*start..*end),
                _ => None,
            })
            .filter(|piece| {
                self.contains(ListRow(piece.start)) != other.contains(ListRow(piece.start))
            });
        Self::from_ranges(differing.collect())
    }

    fn first_ending_after(&self, row: usize) -> usize {
        self.0.partition_point(|range| range.end <= row)
    }
}

fn spanning(ranges: &[Range<usize>]) -> Option<Range<usize>> {
    Some(ranges.first()?.start..ranges.last()?.end)
}

fn union(first: &Range<usize>, second: &Range<usize>) -> Range<usize> {
    first.start.min(second.start)..first.end.max(second.end)
}

fn merge_into(mut merged: Vec<Range<usize>>, range: Range<usize>) -> Vec<Range<usize>> {
    match merged.last_mut() {
        Some(last) if range.start <= last.end => last.end = last.end.max(range.end),
        _ => merged.push(range),
    }
    merged
}
