// © 2019, ETH Zurich
//
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

use encoder::foldunfold::perm::*;
use encoder::vir;
use encoder::vir::PermAmount;
use encoder::vir::ExprIterator;
use std::collections::HashSet;
use std::collections::HashMap;
use std::fmt;
use std::iter::FromIterator;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct State {
    /// paths on which we (may) have a full access permission
    acc: HashMap<vir::Expr, PermAmount>,
    /// paths on which we (may) have a full predicate permission
    pred: HashMap<vir::Expr, PermAmount>,
    /// paths that have been "moved out" (for sure)
    moved: HashSet<vir::Expr>,
    /// Permissions currently framed
    framing_stack: Vec<PermSet>,
    /// Permissions that should be removed from the state
    /// This is a hack for restoring borrows
    dropped: HashSet<Perm>,
}

impl State {
    pub fn new(
        acc: HashMap<vir::Expr, PermAmount>,
        pred: HashMap<vir::Expr, PermAmount>,
        moved: HashSet<vir::Expr>
    ) -> Self {
        State {
            acc,
            pred,
            moved,
            framing_stack: vec![],
            dropped: HashSet::new(),
        }
    }

    pub fn check_consistency(&self) {
        // Check access permissions
        for place in self.pred.keys() {
            if place.is_simple_place() && !self.contains_acc(place) {
                let contains_parent_pred = if let Some(parent) = place.get_parent() {
                    self.pred.contains_key(&parent)
                } else {
                    false
                };
                if !contains_parent_pred &&
                    self.pred[place] != PermAmount::Remaining &&
                    self.pred[place] != PermAmount::Read &&
                    !place.is_mir_reference() {
                    trace!("place: {:?}", place);
                    trace!("Acc state: {{\n{}\n}}", self.display_acc());
                    trace!("Pred state: {{\n{}\n}}", self.display_pred());
                    panic!(
                        "Consistency error: state has pred {}, but not acc {}",
                        place,
                        place
                    );
                }
            }
        }
        for place in self.acc.keys() {
            if place.is_simple_place() && !place.is_local() {
                let parent = place.clone().get_parent().unwrap();
                if !self.contains_acc(&parent) {
                    if self.acc[place] == PermAmount::Read {
                        let grand_parent = parent.clone().get_parent().unwrap();
                        if grand_parent.is_local() {
                            continue;
                        }
                    }
                    panic!(
                        "Consistency error: state has acc {}, but not acc {}",
                        place,
                        place.get_parent().unwrap()
                    );
                }
            }
        }
        // Check predicates and moved paths
        for place in self.pred.keys() {
            for other_place in self.pred.keys() {
                if place.is_simple_place() && other_place.is_simple_place() &&
                    place.has_proper_prefix(&other_place) {
                    if !(self.pred[place] == PermAmount::Read &&
                             self.pred[other_place] == PermAmount::Read) {
                        panic!(
                            "Consistency error: state has pred {} ({}), but also pred {} ({})",
                            place,
                            self.pred[place],
                            other_place,
                            self.pred[other_place]
                        );
                    }
                }
            }
        }
        for acc_place in self.acc.keys() {
            for pred_place in self.pred.keys() {
                if acc_place.is_simple_place() &&
                        pred_place.is_simple_place() &&
                        acc_place.has_proper_prefix(&pred_place) {
                    panic!(
                        "Consistency error: state has acc {}, but also pred {}",
                        acc_place,
                        pred_place
                    );
                }
            }
        }
        for acc_place in self.acc.keys() {
            for moved_place in &self.moved {
                if moved_place.is_simple_place() &&
                        acc_place.is_simple_place() &&
                        acc_place.has_proper_prefix(moved_place) {
                    panic!(
                        "Consistency error: state has acc {}, but also moved path {}",
                        acc_place,
                        moved_place
                    );
                }
            }
        }
        for pred_place in self.pred.keys() {
            for moved_place in &self.moved {
                if moved_place.is_simple_place() &&
                        pred_place.is_simple_place() &&
                        pred_place.has_prefix(moved_place) {
                    panic!(
                        "Consistency error: state has pred {}, but also moved path {}",
                        pred_place,
                        moved_place
                    );
                }
                if moved_place.is_simple_place() &&
                        pred_place.is_simple_place() &&
                        moved_place.has_prefix(pred_place) {
                    panic!(
                        "Consistency error: state has pred {}, but also moved path {}",
                        pred_place,
                        moved_place
                    );
                }
            }
        }
        // Check moved
        for place in &self.moved {
            if place.is_simple_place() && !self.contains_acc(place) &&
                !place.is_mir_reference() &&
                !self.framing_stack.iter().any(|fs|
                    fs.contains(&Perm::Acc(place.clone(), PermAmount::Write))
                ) {
                panic!(
                    "Consistency error: state has moved path {}, but not acc {} (not even a framed one)",
                    place,
                    place
                );
            }
        }
    }

    pub fn replace_local_vars<F>(&mut self, replace: F) where F: Fn(&vir::LocalVar) -> vir::LocalVar {
        for coll in vec![&mut self.acc, &mut self.pred] {
            let new_values = coll.clone().into_iter()
                .map(|(p, perm)| {
                    let base_var = p.get_base();
                    let new_base_var = replace(&base_var);
                    let new_place = p.clone().replace_place(
                        &vir::Expr::local(base_var.clone()),
                        &new_base_var.into());
                    (new_place, perm)
                });
            coll.clear();
            for (key, value) in new_values {
                coll.insert(key, value);
            }
        }

        for coll in vec![&mut self.moved] {
            let new_values = coll.clone().into_iter().map(
                |place| {
                    let base_var = place.get_base();
                    let new_base_var = replace(&base_var);
                    place.clone().replace_place(
                        &vir::Expr::local(base_var.clone()),
                        &new_base_var.into())
                }
            );
            coll.clear();
            for item in new_values {
                coll.insert(item);
            }
        }
    }

    pub fn replace_places<F>(&mut self, replace: F) where F: Fn(vir::Expr) -> vir::Expr {
        for coll in vec![&mut self.acc, &mut self.pred] {
            let new_values = coll.clone().into_iter()
                .map(|(place, perm)| {
                    (replace(place), perm)
                });
            coll.clear();
            for (key, value) in new_values {
                coll.insert(key, value);
            }
        }
    }

    pub fn acc(&self) -> &HashMap<vir::Expr, PermAmount> {
        &self.acc
    }

    pub fn acc_places(&self) -> HashSet<vir::Expr> {
        self.acc.keys().cloned().collect()
    }

    pub fn acc_leaves(&self) -> HashSet<vir::Expr> {
        let mut acc_leaves = HashSet::new();
        for place in self.acc.keys() {
            if !self.is_proper_prefix_of_some_acc(place) {
                acc_leaves.insert(place.clone());
            }
        }
        acc_leaves
    }

    pub fn pred(&self) -> &HashMap<vir::Expr, PermAmount> {
        &self.pred
    }

    pub fn pred_places(&self) -> HashSet<vir::Expr> {
        self.pred.keys().cloned().collect()
    }

    pub fn moved(&self) -> &HashSet<vir::Expr> {
        &self.moved
    }

    pub fn framing_stack(&self) -> &Vec<PermSet> {
        &self.framing_stack
    }

    pub fn set_acc(&mut self, acc: HashMap<vir::Expr, PermAmount>) {
        self.acc = acc
    }

    pub fn set_pred(&mut self, pred: HashMap<vir::Expr, PermAmount>) {
        self.pred = pred
    }

    pub fn set_moved(&mut self, moved: HashSet<vir::Expr>) {
        self.moved = moved
    }

    pub fn contains_acc(&self, place: &vir::Expr) -> bool {
        self.acc.contains_key(&place)
    }

    pub fn contains_pred(&self, place: &vir::Expr) -> bool {
        self.pred.contains_key(&place)
    }

    pub fn contains_moved(&self, place: &vir::Expr) -> bool {
        self.moved.contains(&place)
    }

    /// Note: the permission amount is currently ignored
    pub fn contains_perm(&self, item: &Perm) -> bool {
        match item {
            &Perm::Acc(ref place, _) => self.contains_acc(item.get_place()),
            &Perm::Pred(ref place, _) => self.contains_pred(item.get_place()),
        }
    }

    pub fn contains_all_perms<'a, I>(&mut self, mut items: I) -> bool where I: Iterator<Item = &'a Perm> {
        items.all(|x| self.contains_perm(x))
    }

    pub fn is_proper_prefix_of_some_acc(&self, prefix: &vir::Expr) -> bool {
        for place in self.acc.keys() {
            if place.has_proper_prefix(prefix) {
                return true;
            }
        }
        false
    }

    pub fn is_proper_prefix_of_some_pred(&self, prefix: &vir::Expr) -> bool {
        for place in self.pred.keys() {
            if place.has_proper_prefix(prefix) {
                return true;
            }
        }
        false
    }

    pub fn is_proper_prefix_of_some_moved(&self, prefix: &vir::Expr) -> bool {
        for place in &self.moved {
            if place.has_prefix(prefix) {
                return true;
            }
        }
        false
    }

    pub fn is_prefix_of_some_acc(&self, prefix: &vir::Expr) -> bool {
        for place in self.acc.keys() {
            if place.has_prefix(prefix) {
                return true;
            }
        }
        false
    }

    pub fn is_prefix_of_some_pred(&self, prefix: &vir::Expr) -> bool {
        for place in self.pred.keys() {
            if place.has_prefix(prefix) {
                return true;
            }
        }
        false
    }

    pub fn is_prefix_of_some_moved(&self, prefix: &vir::Expr) -> bool {
        for place in &self.moved {
            if place.has_prefix(prefix) {
                return true;
            }
        }
        false
    }

    pub fn intersect_acc(&mut self, acc_set: &HashSet<vir::Expr>) {
        let mut new_acc = HashMap::new();
        for (place, perm) in self.acc.drain() {
            if acc_set.contains(&place) {
                new_acc.insert(place, perm);
            }
        }
        self.acc = new_acc;
    }

    pub fn intersect_pred(&mut self, pred_set: &HashSet<vir::Expr>) {
        let mut new_pred = HashMap::new();
        for (place, perm) in self.pred.drain() {
            if pred_set.contains(&place) {
                new_pred.insert(place, perm);
            }
        }
        self.pred = new_pred;
    }

    pub fn intersect_moved(&mut self, other_moved: &HashSet<vir::Expr>) {
        self.moved = HashSet::from_iter(self.moved.intersection(other_moved).cloned());
    }

    pub fn remove_all(&mut self) {
        self.remove_matching_place(|_| true);
    }

    pub fn remove_matching_place<P>(&mut self, pred: P)
        where P: Fn(&vir::Expr) -> bool
    {
        self.remove_acc_matching(|x| pred(x));
        self.remove_pred_matching(|x| pred(x));
        self.remove_moved_matching(|x| pred(x));
    }

    pub fn remove_acc_matching<P>(&mut self, pred: P)
        where P: Fn(&vir::Expr) -> bool
    {
        self.acc.retain(|e, _| !pred(e));
    }

    pub fn remove_pred_matching<P>(&mut self, pred: P)
        where P: Fn(&vir::Expr) -> bool
    {
        self.pred.retain(|e, _| !pred(e));
    }

    pub fn remove_moved_matching<P>(&mut self, pred: P)
        where P: Fn(&vir::Expr) -> bool
    {
        self.moved.retain(|e| !pred(e));
    }

    pub fn display_acc(&self) -> String {
        let mut info = self.acc.iter()
            .map(|(p, f)| format!("  {}: {}", p, f))
            .collect::<Vec<String>>();
        info.sort();
        info.join(",\n")
    }

    pub fn display_pred(&self) -> String {
        let mut info = self.pred.iter()
            .map(|(p, f)| format!("  {}: {}", p, f))
            .collect::<Vec<String>>();
        info.sort();
        info.join(",\n")
    }

    pub fn display_moved(&self) -> String {
        let mut info = self.moved.iter()
            .map(|x| format!("  {}", x))
            .collect::<Vec<String>>();
        info.sort();
        info.join(",\n")
    }

    pub fn display_debug_acc(&self) -> String {
        let mut info = self.acc.iter()
            .map(|(place, perm)| format!("  ({:?}, {})", place, perm))
            .collect::<Vec<String>>();
        info.sort();
        info.join(",\n")
    }

    pub fn display_debug_pred(&self) -> String {
        let mut info = self.pred.iter()
            .map(|(place, perm)| format!("  ({:?}, {})", place, perm))
            .collect::<Vec<String>>();
        info.sort();
        info.join(",\n")
    }

    pub fn display_debug_moved(&self) -> String {
        let mut info = self.moved.iter()
            .map(|x| format!("  {:?}", x))
            .collect::<Vec<String>>();
        info.sort();
        info.join(",\n")
    }

    pub fn insert_acc(&mut self, place: vir::Expr, perm: PermAmount) {
        trace!("insert_acc {}, {}", place, perm);
        if self.acc.contains_key(&place) {
            let new_perm = self.acc[&place] + perm;
            assert!(new_perm == PermAmount::Write || new_perm == PermAmount::Read,
                    "Trying to inhale {} access permission, while there is already {}",
                    perm, self.acc[&place]);
            self.acc.insert(place, new_perm);
        } else {
            self.acc.insert(place, perm);
        }
    }

    pub fn insert_all_acc<I>(&mut self, items: I) where I: Iterator<Item = (vir::Expr, PermAmount)> {
        for (place, perm) in items {
            self.insert_acc(place, perm);
        }
    }

    pub fn insert_pred(&mut self, place: vir::Expr, perm: PermAmount) {
        trace!("insert_pred {}, {}", place, perm);
        if self.pred.contains_key(&place) {
            let new_perm = self.pred[&place] + perm;
            assert!(new_perm == PermAmount::Write || new_perm == PermAmount::Read,
                    "Trying to inhale {} predicate permission, while there is already {}",
                    perm, self.pred[&place]);
            self.pred.insert(place, new_perm);
        } else {
            self.pred.insert(place, perm);
        }
    }

    pub fn insert_all_pred<I>(&mut self, items: I) where I: Iterator<Item = (vir::Expr, PermAmount)> {
        for (place, perm) in items {
            self.insert_pred(place, perm);
        }
    }

    pub fn insert_moved(&mut self, place: vir::Expr) {
        //assert!(!self.pred.contains(&place), "Place {} is already in state (pred), so it can not be added.", place);
        self.moved.insert(place);
    }

    pub fn insert_all_moved<I>(&mut self, items: I) where I: Iterator<Item = vir::Expr> {
        for item in items {
            self.insert_moved(item);
        }
    }

    pub fn insert_dropped(&mut self, item: Perm) {
        self.dropped.insert(item);
    }

    pub fn is_dropped(&self, item: &Perm) -> bool {
        self.dropped.contains(item)
    }

    /// Argument: the places to preserve
    pub fn remove_dropped(&mut self, places: &[vir::Expr]) {
        debug_assert!(places.iter().all(|p| p.is_place()));
        let mut to_remove: Vec<_> = self.dropped.iter().cloned().collect();
        self.dropped.clear();
        for dropped_perm in to_remove.into_iter() {
            if !places.iter().any(|p| dropped_perm.get_place().has_prefix(p)) {
                if dropped_perm.is_pred() {
                    self.remove_pred_matching(|p| p.has_prefix(dropped_perm.get_place()));
                }
                if dropped_perm.is_acc() {
                    self.remove_acc_matching(|p| p.has_prefix(dropped_perm.get_place()));
                }
                //self.remove_perm(&dropped_perm);
                //self.remove_moved_matching(|p| p.has_prefix(dropped_perm.get_place()));
                //self.insert_moved(dropped_perm.get_place().clone());
            }
        }
    }

    pub fn insert_perm(&mut self, item: Perm) {
        match item {
            Perm::Acc(place, perm) => self.insert_acc(place, perm),
            Perm::Pred(place, perm) => self.insert_pred(place, perm),
        };
    }

    pub fn insert_all_perms<I>(&mut self, items: I) where I: Iterator<Item=Perm> {
        for item in items {
            self.insert_perm(item);
        }
    }

    pub fn remove_acc_place(&mut self, place: &vir::Expr) -> PermAmount {
        assert!(self.acc.contains_key(place),
                "Place {} is not in state (acc), so it can not be removed.", place);
        self.acc.remove(place).unwrap()
    }

    pub fn remove_pred_place(&mut self, place: &vir::Expr) -> PermAmount {
        assert!(self.pred.contains_key(place),
                "Place {} is not in state (pred), so it can not be removed.", place);
        self.pred.remove(place).unwrap()
    }

    pub fn remove_acc(&mut self, place: &vir::Expr, perm: PermAmount) {
        assert!(self.acc.contains_key(place),
                "Place {} is not in state (acc), so it can not be removed.", place);
        if self.acc[place] == perm {
            self.acc.remove(place);
        } else {
            self.acc.insert(place.clone(), self.acc[place] - perm);
        }
    }

    pub fn remove_pred(&mut self, place: &vir::Expr, perm: PermAmount) {
        trace!("remove_pred {}, {}", place, perm);
        assert!(self.pred.contains_key(place),
                "Place {} is not in state (pred), so it can not be removed.", place);
        if self.pred[place] == perm {
            self.pred.remove(place);
        } else {
            self.pred.insert(place.clone(), self.pred[place] - perm);
        }
    }

    pub fn remove_moved(&mut self, place: &vir::Expr) {
        assert!(self.moved.contains(place), "Place {} is not in state (moved), so it can not be removed.", place);
        self.moved.remove(place);
    }

    pub fn remove_perm(&mut self, item: &Perm) {
        match item {
            &Perm::Acc(_, perm) => self.remove_acc(item.get_place(), perm),
            &Perm::Pred(_, perm) => self.remove_pred(item.get_place(), perm)
        };
    }

    pub fn remove_all_perms<'a, I>(&mut self, items: I) where I: Iterator<Item = &'a Perm> {
        for item in items {
            self.remove_perm(item);
        }
    }

    /// Restores the provided permission. It could be that the dropped
    /// permission is already in the state, for example, if the variable
    /// was assigned again as `x` in the following example:
    ///
    /// ```rust
    /// // pub fn test2(cond1: bool, mut a: ListNode) {
    /// //     let mut x = &mut a;
    /// //     if cond1 {
    /// //         x = match x.next {
    /// //             Some(box ref mut node) => node,
    /// //             None => x,
    /// //         };
    /// //     } // a.value is dropped during the merge.
    /// //     x.value.g.f = 4;
    /// // }
    /// ```
    /// In such a case, the function keeps the most generic variant of
    /// permissions.
    pub fn restore_dropped_perm(&mut self, item: Perm) {
        trace!("[enter] restore_dropped_perm item={}", item);
        for moved_place in &self.moved {
            trace!("  moved_place={}", moved_place);
        }
        match item {
            Perm::Acc(place, perm) => {
                self.remove_moved_matching(|p| place.has_prefix(p));
                self.restore_acc(place, perm);
            },
            Perm::Pred(place, perm) => {
                self.remove_moved_matching(|p| place.has_prefix(p));
                self.restore_pred(place, perm);
            },
        };
        trace!("[exit] restore_dropped_perm");
    }

    fn restore_acc(&mut self, acc_place: vir::Expr, perm: PermAmount) {
        trace!("restore_acc {}, {}", acc_place, perm);
        if acc_place.is_simple_place() {
            for pred_place in self.pred.keys() {
                if  pred_place.is_simple_place() && acc_place.has_proper_prefix(&pred_place) {
                    trace!("restore_acc {}: ignored (predicate already exists: {})",
                           acc_place, pred_place);
                    return;
                }
            }
        }
        if self.acc.contains_key(&acc_place) {
            trace!("restore_acc {}: ignored (state already contains place)", acc_place);
            return;
        }
        self.acc.insert(acc_place, perm);
    }

    fn restore_pred(&mut self, pred_place: vir::Expr, mut perm: PermAmount) {
        trace!("restore_pred {}, {}", pred_place, perm);
        if let Some(curr_perm_amount) = self.pred.get(&pred_place) {
            perm = perm + *curr_perm_amount;
            //trace!("restore_pred {}: ignored (state already contains place)", pred_place);
            //return;
        }
        if pred_place.is_simple_place() {
            self.acc.retain(|acc_place, _| {
                if  acc_place.is_simple_place() && acc_place.has_proper_prefix(&pred_place) {
                    trace!("restore_pred {}: drop conflicting acc {}",
                           pred_place, acc_place);
                    false
                } else {
                    true
                }
            });
        }
        self.pred.insert(pred_place, perm);
    }

    pub fn restore_dropped_perms<I>(&mut self, items: I) where I: Iterator<Item=Perm> {
        trace!("[enter] restore_dropped_perms");
        for item in items {
            self.restore_dropped_perm(item);
        }
        self.check_consistency();
        trace!("[exit] restore_dropped_perms");
    }

    pub fn as_vir_expr(&self) -> vir::Expr {
        let mut exprs: Vec<vir::Expr> = vec![];
        for (place, perm) in self.acc.iter() {
            if !place.is_local() && place.is_curr() {
                if !self.is_dropped(&Perm::acc(place.clone(), *perm)) {
                    exprs.push(
                        vir::Expr::acc_permission(place.clone(), *perm)
                    );
                }
            }
        }
        for (place, perm_amount) in self.pred.iter() {
            if let Some(perm) = vir::Expr::pred_permission(place.clone(), *perm_amount) {
                if !self.is_dropped(&Perm::pred(place.clone(), *perm_amount)) && place.is_curr() {
                    exprs.push(perm);
                }
            }
        }
        exprs.into_iter().conjoin()
    }

    pub fn begin_frame(&mut self) {
        trace!("begin_frame");
        trace!("Before: {} frames are on the stack", self.framing_stack.len());
        let mut framed_perms = PermSet::empty();
        for (place, perm) in self.acc.clone().into_iter() {
            if !place.is_local() {
                self.acc.remove(&place);
                framed_perms.add(Perm::Acc(place.clone(), perm));
            }
        }
        for (place, perm) in self.pred.drain() {
            framed_perms.add(Perm::Pred(place.clone(), perm));
        }
        debug!("Framed permissions: {}", framed_perms);
        self.framing_stack.push(framed_perms);
        trace!("After: {} frames are on the stack", self.framing_stack.len());
    }

    pub fn end_frame(&mut self) {
        trace!("end_frame");
        trace!("Before: {} frames are on the stack", self.framing_stack.len());
        let mut framed_perms = self.framing_stack.pop().unwrap();
        debug!("Framed permissions: {}", framed_perms);
        for perm in framed_perms.perms().drain(..) {
            self.insert_perm(perm);
        }

        trace!("After: {} frames are on the stack", self.framing_stack.len());
    }
}

impl fmt::Display for State {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        writeln!(f, "acc: {{")?;
        writeln!(f, "  {}", self.display_acc())?;
        writeln!(f, "}}")?;
        writeln!(f, "pred: {{")?;
        writeln!(f, "  {}", self.display_pred())?;
        writeln!(f, "}}")
    }
}
