use prusti_contracts::*;
use std::ptr;

pub struct VecWrapperI32 {
    _ghost_size: usize,
    v: Vec<isize>,
}

impl VecWrapperI32 {
    #[trusted]
    #[pure]
    #[ensures (0 <= result)]
    pub fn len(&self) -> isize {
        self._ghost_size as isize
    }

    #[trusted]
    #[requires(size > 0)]
    #[ensures (result.len() == size)]
    #[ensures (forall(|i: isize| (0 <= i && i < result.len()) ==> result.lookup(i) == 0))]
    pub fn new(size: isize) -> Self {
        Self {
            _ghost_size: size as usize,
            v: vec![0; size as usize],
        }
    }

    #[trusted]
    #[pure]
    #[requires (0 <= index && index < self.len())]
    pub fn lookup(&self, index: isize) -> isize {
        self.v[index as usize]
    }

    #[trusted]
    #[requires(0 <= idx && idx < self.len())]
    #[ensures(self.len() == old(self.len()))]
    #[ensures(self.lookup(idx) == value)]
    #[ensures(forall(|i: isize|
        (0 <= i && i < self.len() && i != idx) ==>
        self.lookup(i) == old(self.lookup(i))))]
    pub fn set(&mut self, idx: isize, value: isize) -> () {
        self.v[idx as usize] = value
    }
}

#[pure]
#[trusted]
#[requires(power_of_two(len))]
#[requires(idx >= 1 && idx < len * 2)]
#[ensures(result >= 1)]
#[ensures(power_of_two(result))]
#[ensures(idx == 1 ==> result == len)]
#[ensures(idx >  1 ==> result == range_length(idx / 2, len) / 2)]
#[ensures(idx < len  ==> result > 1)]
#[ensures(idx >= len ==> result == 1)]
fn range_length(idx: isize, len: isize) -> isize {
    if idx == 1 {
        len
    } else {
        range_length(idx / 2, len) / 2
    }
}

#[pure]
#[requires(power_of_two(array.len()))]
#[requires(power_of_two(rIdx - lIdx))]
#[requires(segTree.len() == array.len() * 2)]
#[requires(idx >= 1 && idx < segTree.len())]
#[requires(lIdx >= 0 && lIdx < rIdx && rIdx <= array.len())]
#[requires(rIdx - lIdx == range_length(idx, array.len()))]
#[ensures(result ==> segTree.lookup(idx) == array_range_sum(array,lIdx, rIdx))]
fn sum_property(
    array: &VecWrapperI32,
    segTree: &VecWrapperI32,
    idx: isize,
    lIdx: isize,
    rIdx: isize,
) -> bool {
    if idx >= array.len() {
        segTree.lookup(idx) == array.lookup(lIdx)
    } else {
        assert!((rIdx + lIdx) % 2 == 0);
        let mid = (rIdx + lIdx) / 2;
        assert!(mid * 2 == rIdx + lIdx);
        assert!((rIdx - mid) * 2 == (rIdx - lIdx));
        assert!((idx * 2 + 1) / 2 == idx);
        sum_property(array, segTree, idx * 2, lIdx, mid)
            && sum_property(array, segTree, idx * 2 + 1, mid, rIdx)
            && segTree.lookup(idx) == segTree.lookup(idx * 2) + segTree.lookup(idx * 2 + 1)
    }
}

#[pure]
#[ensures(result <= a && result <= b)]
#[ensures(result == a || result == b)]
fn min(a: isize, b: isize) -> isize {
    if a < b {
        a
    } else {
        b
    }
}

#[pure]
#[ensures(result >= a && result >= b)]
#[ensures(result == a || result == b)]
fn max(a: isize, b: isize) -> isize {
    if a > b {
        a
    } else {
        b
    }
}

#[pure]
#[requires(power_of_two(array.len()))]
#[requires(power_of_two(nodeRIdx - nodeLIdx))]
#[requires(segTree.len() == array.len() * 2)]
#[requires(idx >= 1 && idx < segTree.len())]
#[requires(nodeLIdx >= 0 && nodeLIdx < nodeRIdx && nodeRIdx <= array.len())]
#[requires(nodeRIdx - nodeLIdx == range_length(idx, array.len()))]
#[requires(sum_property(array, segTree, idx, nodeLIdx, nodeRIdx))]
#[requires(lIdx >= nodeLIdx && lIdx < rIdx && rIdx <= nodeRIdx)]
#[ensures(result == array_range_sum(array, lIdx, rIdx))]
fn range_sum(
    segTree: &VecWrapperI32,
    idx: isize,
    lIdx: isize,
    rIdx: isize,
    array: &VecWrapperI32,
    nodeLIdx: isize,
    nodeRIdx: isize,
) -> isize {
    if lIdx == nodeLIdx && rIdx == nodeRIdx {
        segTree.lookup(idx)
    } else {
        let mut result = 0;

        assert!((nodeRIdx + nodeLIdx) % 2 == 0);
        let mid = (nodeRIdx + nodeLIdx) / 2;
        assert!(mid * 2 == nodeRIdx + nodeLIdx);
        assert!((nodeRIdx - mid) * 2 == (nodeRIdx - nodeLIdx));

        if lIdx < mid {
            result += range_sum(segTree, idx * 2, lIdx, min(mid, rIdx), array, nodeLIdx, mid);
        } else {
            result += 0;
        }

        if rIdx > mid {
            result += range_sum(
                segTree,
                idx * 2 + 1,
                max(mid, lIdx),
                rIdx,
                array,
                mid,
                nodeRIdx,
            );
        } else {
            result += 0;
        }

        result
    }
}

#[requires(array.len() > 0)]
#[requires(power_of_two(array.len()))]
#[ensures(power_of_two(segTree.len()))]
#[requires(segTree.len() == 2 * array.len())]
#[requires(idx >= 1 && idx <= segTree.len())]
#[ensures(segTree.len() == 2 * array.len())]
#[ensures(forall(|i:isize| (i >= idx && i < segTree.len() && i >= array.len()) ==> segTree.lookup(i) == array.lookup(i - array.len())))]
#[ensures(forall(|i:isize| (i >= idx && i < segTree.len() && i < array.len()) ==> segTree.lookup(i) == segTree.lookup(i * 2) + segTree.lookup(i * 2 + 1)))]
fn build(array: &VecWrapperI32, segTree: &mut VecWrapperI32, idx: isize) {
    if idx == segTree.len() {

    } else if idx >= array.len() {
        build(array, segTree, idx + 1);
        segTree.set(idx, array.lookup(idx - array.len()));
    } else {
        build(array, segTree, idx + 1);
        let v = segTree.lookup(idx * 2) + segTree.lookup(idx * 2 + 1);
        segTree.set(idx, v);
    }
}

// #[requires(array.len() > 0)]
// #[requires(power_of_two(array.len()))]
// #[ensures(power_of_two(segTree.len()))]
// #[ensures(power_of_two(rIdx - lIdx))]
// #[requires(segTree.len() == 2 * array.len())]
// #[requires(idx >= 1 && idx < segTree.len())]
// #[requires(lIdx >= 0 && lIdx < rIdx && rIdx <= array.len())]
// #[requires(rIdx - lIdx == range_length(idx, array.len()))]
// #[ensures(segTree.len() == 2 * array.len())]
// #[ensures(sum_property(array, segTree, 1, 0, array.len()))]
// fn build(array: &VecWrapperI32, segTree: &mut VecWrapperI32, idx: isize, lIdx: isize, rIdx: isize) {
//     if idx >= array.len() {
//         segTree.set(idx, array.lookup(lIdx));
//     } else {
//         assert!((rIdx + lIdx) % 2 == 0);
//         let mid = (rIdx + lIdx) / 2;
//         assert!(mid * 2 == rIdx + lIdx);
//         assert!((rIdx - mid) * 2 == (rIdx - lIdx));
//         assert!((idx * 2 + 1) / 2 == idx);
//         build(array, segTree, idx * 2, lIdx,  mid);
//         build(array, segTree, idx * 2  + 2, mid,  rIdx);
//         let v = segTree.lookup(idx * 2) + segTree.lookup(idx * 2 + 1);
//         segTree.set(idx, v);
//     }
// }

#[pure]
fn power_of_two(v: isize) -> bool {
    if v == 1 {
        true
    } else {
        let even = (v % 2 == 0);
        even && power_of_two(v / 2)
    }
}

#[pure]
#[requires(lIdx >= 0 && rIdx <= array.len() && lIdx < rIdx)]
#[ensures(lIdx == rIdx - 1 ==> result == array.lookup(lIdx))]
#[ensures(forall(|i: isize| (i > lIdx && i < rIdx) ==> result == array_range_sum(array, lIdx, i) + array_range_sum(array, i, rIdx)))]
fn array_range_sum(array: &VecWrapperI32, lIdx: isize, rIdx: isize) -> isize {
    if lIdx == rIdx - 1 {
        array.lookup(lIdx)
    } else {
        array.lookup(lIdx) + array_range_sum(array, lIdx + 1, rIdx)
    }
}

// #[requires(power_of_two(array.len()))]
// #[requires(lIdx >= 0 && rIdx <= array.len() && lIdx < rIdx)]
// #[ensures(result == array_range_sum(array, lIdx, rIdx))]
// fn solve(array: &VecWrapperI32, lIdx: isize, rIdx: isize) -> isize {
//     let mut segTree = VecWrapperI32::new(2 * array.len());
//     build(array, &mut segTree, 1);
//     assert!(sum_property(array, &segTree, 1,  0, array.len()));
//     range_sum(&segTree, 1, lIdx, rIdx, array, 0, array.len())
// }

fn main() {}
