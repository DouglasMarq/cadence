use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Page<T> {
    pub items: Vec<T>,
    pub start_index: usize,
    pub total_count: Option<usize>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Paging {
    pub start_index: usize,
    pub limit: usize,
}

impl<T> Page<T> {
    pub fn from_all<Items>(items: Items, paging: Paging) -> Self
    where
        Items: IntoIterator<Item = T>,
        Items::IntoIter: ExactSizeIterator,
    {
        let items = items.into_iter();
        let total = items.len();
        let page_size = match paging.limit {
            0 => total,
            limit => limit,
        };
        Self {
            items: items.skip(paging.start_index).take(page_size).collect(),
            start_index: paging.start_index,
            total_count: Some(total),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn paging(start_index: usize, limit: usize) -> Paging {
        Paging { start_index, limit }
    }

    #[test]
    fn given_a_full_list_when_paging_then_the_page_is_sliced_and_counts_every_item() {
        let page = Page::from_all(vec![1, 2, 3, 4], paging(1, 2));
        assert_eq!(page.items, [2, 3]);
        assert_eq!(page.start_index, 1);
        assert_eq!(page.total_count, Some(4));
    }

    #[test]
    fn given_a_zero_limit_when_paging_then_every_remaining_item_is_returned() {
        let page = Page::from_all(vec![1, 2, 3, 4], paging(2, 0));
        assert_eq!(page.items, [3, 4]);
    }

    #[test]
    fn given_a_start_past_the_end_when_paging_then_the_page_is_empty() {
        let page = Page::from_all(vec![1, 2], paging(5, 2));
        assert!(page.items.is_empty());
        assert_eq!(page.total_count, Some(2));
    }
}
