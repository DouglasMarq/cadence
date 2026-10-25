## Rules

- Database queries go directly in the repository layer.
- Check every new or changed query for performance — confirm with `EXPLAIN` that it uses an index (when one is missing, flag it and propose the migration — never add it without asking) and does not cause full table scans, N+1 queries or unbounded results.
- The domain model lives in the feature's model file (`order.model.ts`, `order.go`, `Order.java`), never in the service file.
- Design features so a new variant is a new implementation behind an existing interface — adding Mastercard to a payment connector must not rewrite it. Add that interface only where more variants are known to be coming (card brands, payment providers) — ask when unsure. This is the one exception to the no-speculative-abstraction rules. Implement it in the lightest form that fits — a map of functions when each variant is one function; an interface with implementations (classes, structs) when variants have several operations or their own dependencies.

## Anti-patterns

- NEVER access the database from controllers or resolvers.
- NEVER put business rules in the repository layer — it only reads and writes data.
- NEVER create circular module dependencies.
- NEVER chain through another object's structure (`order.getCustomer().getAddress().getCity()`) — receive the value you need, or ask the owner (`order.getShippingAddress()`). Reading plain data objects is fine.
