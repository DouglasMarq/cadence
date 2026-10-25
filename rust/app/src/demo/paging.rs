use cadence_core::model::{Page, Paging};

pub(super) fn paginate<Item>(
    items: impl ExactSizeIterator<Item = Item>,
    paging: Paging,
) -> Page<Item> {
    Page::from_all(items, paging)
}

pub(super) fn paginate_order<Item>(
    order: &[usize],
    paging: Paging,
    to_item: impl Fn(usize) -> Item,
) -> Page<Item> {
    let page = Page::from_all(order.iter().copied(), paging);
    Page {
        items: page.items.into_iter().map(to_item).collect(),
        start_index: page.start_index,
        total_count: page.total_count,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const ALL: usize = 0;

    #[test]
    fn given_a_zero_limit_when_paginating_then_every_remaining_item_is_returned() {
        let page = paginate(
            [1, 2, 3, 4].into_iter(),
            Paging {
                start_index: 1,
                limit: ALL,
            },
        );
        assert_eq!(page.items, [2, 3, 4]);
        assert_eq!(page.total_count, Some(4));
    }

    #[test]
    fn given_a_zero_limit_when_paginating_an_order_then_every_remaining_item_is_returned() {
        let page = paginate_order(
            &[3, 1, 2],
            Paging {
                start_index: 1,
                limit: ALL,
            },
            |index| index * 10,
        );
        assert_eq!(page.items, [10, 20]);
        assert_eq!(page.total_count, Some(3));
    }
}
