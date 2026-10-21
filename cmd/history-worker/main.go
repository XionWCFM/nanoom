//go:build js && wasm

package main

import (
	"context"
	"database/sql"
	"log"

	"github.com/XionWCFM/nanoom/internal/history"
	"github.com/syumai/workers-go"
	"github.com/syumai/workers-go/cloudflare"
	"github.com/syumai/workers-go/cloudflare/cron"
	"github.com/syumai/workers-go/cloudflare/d1"
)

func main() {
	server := &history.Server{}
	auth, err := history.ParseAuth([]byte(cloudflare.Getenv("NANOOM_AUTH_JSON")))
	if err == nil {
		server.Auth = auth
	}
	connector, err := d1.OpenConnector("PREDICTION_STATE")
	if err == nil {
		db := sql.OpenDB(connector)
		server.Store = history.SQLStore{DB: db}
	}
	cron.ScheduleTaskNonBlock(func(ctx context.Context) error {
		if _, err := server.Cleanup(ctx); err != nil {
			log.Print("Scheduled history cleanup failed.")
		}
		return nil
	})
	workers.Serve(server)
}
