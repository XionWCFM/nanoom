// Package prediction implements the portable Nanoom PredictionState v3 contract.
package prediction

import (
	"bytes"
	"crypto/sha256"
	"encoding/hex"
	"encoding/json"
	"fmt"
	"io"
	"reflect"
	"sort"
	"strconv"
	"unicode/utf8"
)

// Decode rejects unknown, duplicate and missing required fields, including nested
// objects. Plain encoding/json silently accepts these at a trust boundary.
func Decode(data []byte, value any) error {
	if !utf8.Valid(data) {
		return fmt.Errorf("invalid UTF-8 JSON")
	}
	if err := validateEscapes(data); err != nil {
		return err
	}
	d := json.NewDecoder(bytes.NewReader(data))
	d.UseNumber()
	raw, err := readValue(d)
	if err != nil {
		return err
	}
	if _, err = d.Token(); err != io.EOF {
		return fmt.Errorf("expected a single JSON value")
	}
	if err = checkShape(raw, reflect.TypeOf(value).Elem()); err != nil {
		return err
	}
	d = json.NewDecoder(bytes.NewReader(data))
	d.DisallowUnknownFields()
	return d.Decode(value)
}

func readValue(d *json.Decoder) (any, error) { return readValueDepth(d, 0) }
func readValueDepth(d *json.Decoder, depth int) (any, error) {
	if depth >= 128 {
		return nil, fmt.Errorf("JSON recursion limit exceeded")
	}
	t, err := d.Token()
	if err != nil {
		return nil, err
	}
	switch t {
	case json.Delim('{'):
		m := map[string]any{}
		for d.More() {
			k, err := d.Token()
			if err != nil {
				return nil, err
			}
			key, ok := k.(string)
			if !ok {
				return nil, fmt.Errorf("invalid object key")
			}
			if _, ok := m[key]; ok {
				return nil, fmt.Errorf("duplicate JSON field %q", key)
			}
			v, err := readValueDepth(d, depth+1)
			if err != nil {
				return nil, err
			}
			m[key] = v
		}
		_, err := d.Token()
		return m, err
	case json.Delim('['):
		a := []any{}
		for d.More() {
			v, err := readValueDepth(d, depth+1)
			if err != nil {
				return nil, err
			}
			a = append(a, v)
		}
		_, err := d.Token()
		return a, err
	default:
		return t, nil
	}
}

func checkShape(v any, t reflect.Type) error {
	if t.Kind() == reflect.Pointer {
		if v == nil {
			return nil
		}
		return checkShape(v, t.Elem())
	}
	// Tagged unions and heterogeneous tuples validate their shape in UnmarshalJSON.
	if reflect.PointerTo(t).Implements(reflect.TypeFor[json.Unmarshaler]()) {
		return nil
	}
	if v == nil && t.Kind() != reflect.Interface {
		return fmt.Errorf("null is invalid for %s", t)
	}
	switch t.Kind() {
	case reflect.Struct:
		m, ok := v.(map[string]any)
		if !ok {
			return fmt.Errorf("expected object for %s", t)
		}
		known := map[string]bool{}
		for i := 0; i < t.NumField(); i++ {
			f := t.Field(i)
			if f.PkgPath != "" {
				continue
			}
			name := f.Tag.Get("json")
			for j, c := range name {
				if c == ',' {
					name = name[:j]
					break
				}
			}
			if name == "-" {
				continue
			}
			if name == "" {
				name = f.Name
			}
			known[name] = true
			item, found := m[name]
			if !found {
				if f.Type.Kind() == reflect.Pointer || f.Tag.Get("optional") == "true" {
					continue
				}
				return fmt.Errorf("missing field %q", name)
			}
			if err := checkShape(item, f.Type); err != nil {
				return fmt.Errorf("%s: %w", name, err)
			}
		}
		for name := range m {
			if !known[name] {
				return fmt.Errorf("unknown field %q", name)
			}
		}
	case reflect.Map:
		m, ok := v.(map[string]any)
		if !ok {
			return fmt.Errorf("expected object")
		}
		for name, item := range m {
			if err := checkShape(item, t.Elem()); err != nil {
				return fmt.Errorf("%s: %w", name, err)
			}
		}
	case reflect.Slice, reflect.Array:
		a, ok := v.([]any)
		if !ok {
			return fmt.Errorf("expected array")
		}
		if t.Kind() == reflect.Array && len(a) != t.Len() {
			return fmt.Errorf("expected %d tuple elements", t.Len())
		}
		for _, item := range a {
			if err := checkShape(item, t.Elem()); err != nil {
				return err
			}
		}
	}
	return nil
}

// CanonicalBytes matches serde_json::Value's sorted-object representation.
// Integer tokens stay integers; HTML and Unicode line separators stay literal.
func CanonicalBytes(value any) ([]byte, error) {
	b, err := json.Marshal(value)
	if err != nil {
		return nil, err
	}
	d := json.NewDecoder(bytes.NewReader(b))
	d.UseNumber()
	v, err := readValue(d)
	if err != nil {
		return nil, err
	}
	var out bytes.Buffer
	if err := writeCanonical(&out, v); err != nil {
		return nil, err
	}
	return out.Bytes(), nil
}
func writeCanonical(out *bytes.Buffer, v any) error {
	switch x := v.(type) {
	case map[string]any:
		out.WriteByte('{')
		keys := make([]string, 0, len(x))
		for k := range x {
			keys = append(keys, k)
		}
		sort.Strings(keys)
		for i, k := range keys {
			if i > 0 {
				out.WriteByte(',')
			}
			writeString(out, k)
			out.WriteByte(':')
			if err := writeCanonical(out, x[k]); err != nil {
				return err
			}
		}
		out.WriteByte('}')
	case []any:
		out.WriteByte('[')
		for i, item := range x {
			if i > 0 {
				out.WriteByte(',')
			}
			if err := writeCanonical(out, item); err != nil {
				return err
			}
		}
		out.WriteByte(']')
	case string:
		writeString(out, x)
	case json.Number:
		// v3 contracts contain only integers; reject float/exponent encodings rather
		// than silently change an identity through a lossy float64 conversion.
		if _, err := strconv.ParseInt(string(x), 10, 64); err != nil {
			if _, err := strconv.ParseUint(string(x), 10, 64); err != nil {
				return fmt.Errorf("canonical v3 JSON requires integers: %s", x)
			}
		}
		out.WriteString(string(x))
	case bool:
		out.WriteString(strconv.FormatBool(x))
	case nil:
		out.WriteString("null")
	default:
		return fmt.Errorf("unsupported JSON value %T", v)
	}
	return nil
}
func writeString(out *bytes.Buffer, s string) {
	out.WriteByte('"')
	for _, c := range s {
		switch c {
		case '"', '\\':
			out.WriteByte('\\')
			out.WriteRune(c)
		case '\b':
			out.WriteString(`\b`)
		case '\f':
			out.WriteString(`\f`)
		case '\n':
			out.WriteString(`\n`)
		case '\r':
			out.WriteString(`\r`)
		case '\t':
			out.WriteString(`\t`)
		default:
			if c < 0x20 {
				fmt.Fprintf(out, `\u%04x`, c)
			} else {
				out.WriteRune(c)
			}
		}
	}
	out.WriteByte('"')
}
func HexDigest(b []byte) string { sum := sha256.Sum256(b); return hex.EncodeToString(sum[:]) }
func Digest(value any) (string, error) {
	b, err := CanonicalBytes(value)
	if err != nil {
		return "", err
	}
	return HexDigest(b), nil
}

// encoding/json replaces unpaired UTF-16 surrogates; serde rejects them. Refuse
// that lossy conversion before it can change a scope, key or credential.
func validateEscapes(data []byte) error {
	inside := false
	for i := 0; i < len(data); i++ {
		if data[i] == '"' {
			inside = !inside
			continue
		}
		if !inside || data[i] != '\\' {
			continue
		}
		if i+1 >= len(data) {
			return fmt.Errorf("incomplete JSON escape")
		}
		if data[i+1] != 'u' {
			i++
			continue
		}
		if i+6 > len(data) {
			return fmt.Errorf("incomplete Unicode escape")
		}
		code, err := strconv.ParseUint(string(data[i+2:i+6]), 16, 16)
		if err != nil {
			return err
		}
		if code >= 0xd800 && code <= 0xdbff {
			if i+12 > len(data) || data[i+6] != '\\' || data[i+7] != 'u' {
				return fmt.Errorf("unpaired UTF-16 surrogate")
			}
			low, err := strconv.ParseUint(string(data[i+8:i+12]), 16, 16)
			if err != nil || low < 0xdc00 || low > 0xdfff {
				return fmt.Errorf("unpaired UTF-16 surrogate")
			}
			i += 11
		} else if code >= 0xdc00 && code <= 0xdfff {
			return fmt.Errorf("unpaired UTF-16 surrogate")
		} else {
			i += 5
		}
	}
	return nil
}
