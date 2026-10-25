## Rules

- The description explains what changed.
- The description lists every behavior change with its `file:line`, the behavior before, the behavior after and the reason. If there is none, it says so.

  | Where                                 | Before                                  | After                              | Reason                         |
  | ------------------------------------- | --------------------------------------- | ---------------------------------- | ------------------------------ |
  | `src/payments/payment.service.ts:42`  | Expired cards were sent to the provider | Expired cards are rejected (`402`) | Avoid provider fees on failure |
