// Serves POST /orders/search (spec/paths/orders-search.yml).
export function searchOrders(store, body) {
  const skip = body.skip ?? 0;
  const limit = Math.min(Number(body.limit ?? 50), 200);
  const items = store
    .filter((order) => !body.statuses || body.statuses.includes(order.status))
    .sort((a, b) => b.createdAt.localeCompare(a.createdAt)); // newest first
  return { count: items.length, items: items.slice(skip, skip + limit) };
}
