# Bindings

`council.d.ts` gives the TypeScript types of the council data and the adapter interface. `ports/node`
implements them in JavaScript. The Go, Rust, Java, C#, PHP and Ruby ports define the same interface in
their own language. All of them pass the shared vectors.

To port the engine to another language, follow `spec/PORTING.md`. The adapter is the only part that
touches a model. The engine validates every adapter answer against the contract
(`spec/schema/view.schema.json`, `spec/schema/merge.schema.json`, and the exact rules in
`spec/SEMANTICS.md` section 3) before it uses it.
