package cli

import "testing"

func TestBatchArgumentPreservesMetacharacters(t *testing.T) {
	for _, c := range []struct{ input, want string }{{"", `^"^"`}, {"name & other", `^"name^ ^&^ other^"`}, {`%PATH%!`, `^"^%PATH^%^!^"`}, {`a"b`, `^"a\^"b^"`}, {`C:\tail\`, `^"C:\tail\\^"`}} {
		got, err := batchArgument(c.input)
		if err != nil || got != c.want {
			t.Fatalf("%q: %q != %q (%v)", c.input, got, c.want, err)
		}
	}
	for _, input := range []string{"line\nbreak", "line\rbreak", "null\x00byte"} {
		if _, err := batchArgument(input); err == nil {
			t.Fatal("invalid batch argument accepted")
		}
	}
}
