// Package nanoom exposes the published configuration schema embedded by the CLI.
package nanoom

import _ "embed"

//go:embed nanoom.schema.json
var ConfigSchema []byte
