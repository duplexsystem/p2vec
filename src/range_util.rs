use std::ops::Range;

fn consolidate<Idx>(a: &Range<Idx>, b: &Range<Idx>) -> Option<Range<Idx>>
where
    Idx: PartialOrd + Clone,
{
    if a.start <= b.start {
        if b.end <= a.end {
            Some(a.clone())
        } else if a.end < b.start {
            None
        } else {
            Some(Range {
                start: a.start.clone(),
                end: b.end.clone(),
            })
        }
    } else {
        consolidate(b, a)
    }
}

fn consolidate_all<Idx>(mut ranges: Vec<Range<Idx>>) -> Vec<Range<Idx>>
where
    Idx: PartialOrd + Clone,
{
    // Panics for incomparable elements! So no NaN for floats, for instance.
    //ranges.sort_by(|a, b| a.p);
    glidesort::sort_by(&mut ranges, |a, b| a.start.partial_cmp(&b.start));

    let mut ranges = ranges.into_iter();
    let mut result = Vec::new();

    if let Some(current) = ranges.next() {
        let leftover = ranges.fold(current, |mut acc, next| {
            match consolidate(&acc, &next) {
                Some(merger) => {
                    acc = merger;
                }

                None => {
                    result.push(acc);
                    acc = next;
                }
            }

            acc
        });

        result.push(leftover);
    }

    result
}
