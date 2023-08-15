use crate::{HashArray, HashEntry};
use std::io::{Error, ErrorKind, Write};

pub fn compress_sorted_entries(
    mut entries: impl DoubleEndedIterator<Item = HashEntry<32, 32>>,
    count: u64,
    mut by: impl FnMut(&HashEntry<32, 32>) -> &HashArray<32>,
    writer: &mut impl Write,
) -> std::io::Result<()> {
    //compression algorithm for storing diffs between entries, when they are sorted
    //1. calculate average diff for entries count, (last - first) / length
    //2. store each entry as a difference from previous entry, minus average diff
    //3. encode entries as variable size integers

    let Some(start) = entries.next() else { return Ok(()) };
    //always write first
    writer.write_all(start.id.get_ref())?;
    writer.write_all(start.data.get_ref())?;
    let Some(end) = entries.next_back() else { return Ok(()) };
    //always write last (so that reader can deduce average span)
    writer.write_all(end.id.get_ref())?;
    writer.write_all(end.data.get_ref())?;
    //then write all other entries

    let count = count.checked_sub(1).ok_or(Error::new(ErrorKind::Other, "Invalid count field"))?;
    //calculate average numeric difference
    let span_num = by(&end).wrapping_sub(*by(&start));
    //divide span by count
    let (average_span, _) = span_num
        .checked_div_rem(count)
        .ok_or(Error::new(ErrorKind::Other, "invalid count field - value is too low"))?;

    println!("Diff num: {span_num:?}");
    println!("avg span: {average_span:?}");

    println!("First diffs:");
    let prev = start;
    for (i, e) in entries.take(10).enumerate() {
        let diff = e.id.wrapping_sub(prev.id);
        let normalized = diff.wrapping_sub(average_span);
        println!("[{i}]: {diff:?}  n: {normalized:?}");
    }

    Ok(())
}
