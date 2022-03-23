pub(crate) fn rot13(s: &str) -> String {
    let mut ret = String::with_capacity(s.len());
    for c in s.chars() {
        if c >= 'A' && c <= 'Z' {
            let num = (c as u32) - ('A' as u32);
            let new_num = (num + 13) % 26;
            let new_c = char::from_u32(new_num + ('A' as u32)).unwrap();
            ret.push(new_c);
        } else if c >= 'a' && c <= 'z' {
            let num = (c as u32) - ('a' as u32);
            let new_num = (num + 13) % 26;
            let new_c = char::from_u32(new_num + ('a' as u32)).unwrap();
            ret.push(new_c);
        } else {
            ret.push(c);
        }
    }
    ret
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rot13() {
        assert_eq!(rot13("Ares"), "Nerf");
        assert_eq!(rot13("abjurer"), "nowhere");
        assert_eq!(rot13("balk"), "onyx");
        assert_eq!(rot13("Why did the chicken cross the road?"), "Jul qvq gur puvpxra pebff gur ebnq?");
        assert_eq!(rot13("Gb trg gb gur bgure fvqr!"), "To get to the other side!");
    }
}
