// Copyright 2025 The Oikome Authors
// SPDX-License-Identifier: Apache-2.0

oikome.onData(({ collections, today, language }) => {
  const root = document.getElementById("count");
  root.replaceChildren();
  const rows = collections.flatMap((collection) => collection.rows);
  const first = new Date(`${today}T00:00:00`);
  first.setDate(first.getDate() - 6);
  const recent = rows.filter((row) => {
    const value = row.created_at ?? row.createdAt ?? row.date ?? row.taken;
    const parsed = new Date(String(value ?? ""));
    return !Number.isNaN(parsed.getTime()) && parsed >= first;
  }).length;
  const total = document.createElement("strong");
  total.textContent = new Intl.NumberFormat(language).format(rows.length);
  const label = document.createElement("span");
  label.textContent = "notes";
  const added = document.createElement("p");
  added.textContent = `${new Intl.NumberFormat(language).format(recent)} in the last 7 days`;
  root.append(total, label, added);
});
