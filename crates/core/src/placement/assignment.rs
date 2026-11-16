//! The cheapest way to pair rows with columns, for placing several windows into zones at
//! once.

/// A cost no pairing may use.
pub const FORBIDDEN: i128 = i128::MAX / 4;

/// The column given to each row so that the total cost is the least, for a square matrix of
/// costs (Hungarian method, cubic in the size). Every row must be able to get a column
/// without a `FORBIDDEN` cost. Equal totals go the same way every time.
pub fn cheapest_assignment(costs: &[Vec<i128>]) -> Vec<usize> {
    let size = costs.len();
    // Potentials and the matching are 1-based, with 0 as the unmatched sentinel.
    let mut row_potential = vec![0i128; size + 1];
    let mut column_potential = vec![0i128; size + 1];
    let mut row_of_column = vec![0usize; size + 1];
    let mut previous_column = vec![0usize; size + 1];

    for row in 1..=size {
        row_of_column[0] = row;
        let mut column = 0;
        let mut slack = vec![FORBIDDEN; size + 1];
        let mut visited = vec![false; size + 1];
        loop {
            visited[column] = true;
            let current_row = row_of_column[column];
            let mut delta = FORBIDDEN;
            let mut next_column = 0;
            for candidate in 1..=size {
                if visited[candidate] {
                    continue;
                }
                let reduced = costs[current_row - 1][candidate - 1]
                    - row_potential[current_row]
                    - column_potential[candidate];
                if reduced < slack[candidate] {
                    slack[candidate] = reduced;
                    previous_column[candidate] = column;
                }
                if slack[candidate] < delta {
                    delta = slack[candidate];
                    next_column = candidate;
                }
            }
            for candidate in 0..=size {
                if visited[candidate] {
                    row_potential[row_of_column[candidate]] += delta;
                    column_potential[candidate] -= delta;
                } else {
                    slack[candidate] -= delta;
                }
            }
            column = next_column;
            if row_of_column[column] == 0 {
                break;
            }
        }
        while column != 0 {
            let previous = previous_column[column];
            row_of_column[column] = row_of_column[previous];
            column = previous;
        }
    }

    let mut column_of_row = vec![0; size];
    for column in 1..=size {
        if row_of_column[column] != 0 {
            column_of_row[row_of_column[column] - 1] = column - 1;
        }
    }
    column_of_row
}

#[cfg(test)]
mod tests {
    use super::*;

    fn total(costs: &[Vec<i128>], assignment: &[usize]) -> i128 {
        assignment
            .iter()
            .enumerate()
            .map(|(row, column)| costs[row][*column])
            .sum()
    }

    /// Every permutation's total, to check the method against.
    fn cheapest_by_trying_all(costs: &[Vec<i128>]) -> i128 {
        fn walk(costs: &[Vec<i128>], row: usize, used: &mut Vec<bool>, sum: i128, best: &mut i128) {
            if row == costs.len() {
                *best = (*best).min(sum);
                return;
            }
            for column in 0..costs.len() {
                if !used[column] && costs[row][column] < FORBIDDEN {
                    used[column] = true;
                    walk(costs, row + 1, used, sum + costs[row][column], best);
                    used[column] = false;
                }
            }
        }
        let mut best = FORBIDDEN;
        walk(costs, 0, &mut vec![false; costs.len()], 0, &mut best);
        best
    }

    #[test]
    fn the_cheapest_pairing_wins_over_the_greedy_one() {
        // Greedy gives row 0 column 0 (cost 1), then row 1 must take column 1 (cost 100).
        let costs = vec![vec![1, 2], vec![3, 100]];
        assert_eq!(cheapest_assignment(&costs), [1, 0]);
    }

    #[test]
    fn it_matches_trying_every_pairing() {
        let mut seed = 7u64;
        let mut next = || {
            seed = seed
                .wrapping_mul(6364136223846793005)
                .wrapping_add(1442695040888963407);
            i128::from((seed >> 33) % 1000)
        };
        for size in 1..=6 {
            for _ in 0..30 {
                let costs: Vec<Vec<i128>> = (0..size).map(|_| (0..size).map(|_| next()).collect()).collect();
                let assignment = cheapest_assignment(&costs);
                let mut columns = assignment.clone();
                columns.sort_unstable();
                assert_eq!(columns, (0..size).collect::<Vec<_>>(), "a permutation");
                assert_eq!(total(&costs, &assignment), cheapest_by_trying_all(&costs));
            }
        }
    }

    #[test]
    fn forbidden_pairings_are_avoided() {
        let costs = vec![
            vec![FORBIDDEN, 5, 9],
            vec![1, FORBIDDEN, 2],
            vec![4, 3, FORBIDDEN],
        ];
        let assignment = cheapest_assignment(&costs);
        assert!(
            assignment
                .iter()
                .enumerate()
                .all(|(row, column)| costs[row][*column] < FORBIDDEN)
        );
        assert_eq!(total(&costs, &assignment), cheapest_by_trying_all(&costs));
    }

    #[test]
    fn nothing_to_assign_gives_nothing() {
        assert!(cheapest_assignment(&[]).is_empty());
    }
}
