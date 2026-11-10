// Command conformance runs the shared vectors (spec/conformance) against the Go port.
//
//	go run ./cmd/conformance [DIR]
//
// It prints each failure and a last line "go: N/N vectors passed"; the exit status is 1 on any failure.
package main

import (
	"fmt"
	"os"

	sv "github.com/SmartTasksOrg/sf-smartsavant/ports/go/smartsavant"
)

func main() {
	dir := "../../spec/conformance"
	if len(os.Args) > 1 {
		dir = os.Args[1]
	}
	total, fails, err := sv.RunConformance(dir)
	if err != nil {
		fmt.Println(err)
		fmt.Println("go: FAILED")
		os.Exit(1)
	}
	for _, f := range fails {
		fmt.Println(f)
	}
	if len(fails) > 0 {
		fmt.Printf("go: %d/%d vectors passed (%d failed)\n", total-len(fails), total, len(fails))
		os.Exit(1)
	}
	fmt.Printf("go: %d/%d vectors passed\n", total, total)
}
