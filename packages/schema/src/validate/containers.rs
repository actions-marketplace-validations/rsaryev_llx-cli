//! The keywords of arrays and objects: each item and field walked against its node, then the
//! counts. A data object is read once, its fields and `properties` met in one pass in the order
//! of their names; the quick walk knows every required field met by its bit, and looks none up.

use serde_json::{Map, Value};

use super::Walk;
use super::cursor::Cursor;
use crate::hash;
use crate::node::{Arrays, Extra, Objects};
use crate::report::{At, Report, said};

impl Walk<'_> {
    /// `items`, `prefixItems` (or draft 7's `items` as a list with `additionalItems`), `minItems`,
    /// `maxItems`, `uniqueItems`, and `contains` with `minContains` and `maxContains`.
    pub(super) fn items<const F: bool>(
        &self,
        arrays: &Arrays,
        items: &[Value],
        at: &At<'_>,
        r: &mut Report<'_, F>,
    ) -> bool {
        // A quick walk fails on the count before it reads an item; a full one says the items'
        // failures first.
        if !r.full() && !item_counts(arrays, items, at, r) {
            return false;
        }
        // Items that need only a type are read in one loop with no call, their node looked up
        // once; one that fails is walked below, to be said.
        let typed = (arrays.prefix.is_empty())
            .then_some(arrays.rest)
            .flatten()
            .and_then(|rest| self.types_of(rest));
        let settled =
            typed.is_some_and(|types| items.iter().all(|item| self.has_type(types, item)));
        for (index, item) in items.iter().enumerate().filter(|_| !settled) {
            let Some(id) = arrays.prefix.get(index).copied().or(arrays.rest) else {
                break;
            };
            let ok = self.fits(id, item) || self.node(id, item, &At::Index(at, index), r.part(), 0);
            if !r.took(ok) {
                return false;
            }
        }
        if r.full() && !item_counts(arrays, items, at, r) {
            return false;
        }
        if arrays.unique
            && let Some((first, second)) = hash::duplicate(items)
            && !r.no(|| {
                said!("{at} must have unique items ({at}[{first}] and {at}[{second}] are equal)")
            })
        {
            return false;
        }
        let Some((contains, least, most)) = arrays.contains else {
            return true;
        };
        let mut found = 0;
        for (index, item) in items.iter().enumerate() {
            if self.node(contains, item, &At::Index(at, index), Report::quiet(), 0) {
                found += 1;
                if most.is_none() && found >= least {
                    return true;
                }
            }
        }
        match (found < least, most.filter(|most| found > *most)) {
            (true, _) => r.no(|| {
                said!("{at} must have at least {least} items matching contains (has {found})")
            }),
            (false, Some(most)) => r.no(|| {
                said!("{at} must have at most {most} items matching contains (has {found})")
            }),
            (false, None) => true,
        }
    }

    /// `required`, `properties`, `patternProperties`, `additionalProperties`, `propertyNames`,
    /// `minProperties`, `maxProperties`, `dependentRequired` and `dependentSchemas`.
    pub(super) fn fields<const F: bool>(
        &self,
        objects: &Objects,
        object: &Value,
        fields: &Map<String, Value>,
        at: &At<'_>,
        r: &mut Report<'_, F>,
    ) -> bool {
        // A full walk says the missing ones first; a quick one marks them as it meets the fields.
        let bits = objects.required_bits.filter(|_| !r.full());
        if bits.is_none() && !missing(objects, fields, at, r) {
            return false;
        }
        // A quick walk fails on the counts before it reads a field.
        if objects.closing && !r.full() && !counts(objects, fields, at, r) {
            return false;
        }
        let open = matches!(objects.extra, Extra::Absent | Extra::Allowed)
            && objects.patterns.is_empty()
            && objects.names.is_none();
        // Only `properties` look at a field: with more fields than it names, each it names is
        // looked up, and the rest are never read.
        if open && fields.len() > objects.properties.len() {
            let mut met = 0;
            for (name, id, bit) in &objects.properties {
                let Some(value) = fields.get(&**name) else {
                    // A required field looked up and not there: no need to read on.
                    if bits.is_some() && *bit != 0 {
                        return r.no(String::new);
                    }
                    continue;
                };
                met |= bit;
                let ok =
                    self.fits(*id, value) || self.node(*id, value, &At::Key(at, name), r.part(), 0);
                if !r.took(ok) {
                    return false;
                }
            }
            if bits.is_some_and(|bits| met & bits != bits) {
                return r.no(String::new);
            }
            return !objects.closing || self.closing(objects, object, fields, at, r);
        }
        if let Some(ok) = self.only_extra(objects, object, fields, at, r) {
            return ok;
        }
        let mut met = 0;
        let unread = open && objects.properties.is_empty();
        let sparse = objects.properties.len() > fields.len().saturating_mul(4);
        let mut cursor = Cursor {
            properties: &objects.properties,
            next: 0,
            passed: 0,
            last: "",
        };
        for (name, value) in fields.iter().filter(|_| !unread) {
            let here = At::Key(at, name);
            if let Some(names) = objects.names {
                let ok = match self.name(names, name, &here, r.part()) {
                    Some(ok) => ok,
                    // A schema that asks more of a name than a string's keywords reads it whole.
                    None => self.node(names, &Value::String(name.clone()), &here, r.part(), 0),
                };
                if !r.took(ok) {
                    return false;
                }
            }
            // A required field the cursor went past unmet is most likely missing: one lookup
            // says so, and the walk fails there, not after every field.
            if let Some(bits) = bits
                && cursor.passed & !met & bits != 0
                && cursor.missing(fields, met)
            {
                return r.no(String::new);
            }
            let mut named = false;
            let found = match &objects.lookup {
                // The data's fields come in the order of their names, as the cursor reads them:
                // a hash is worth its cost only where the schema names far more than come.
                Some(lookup) if sparse => (lookup.find(&objects.properties, name))
                    .and_then(|place| objects.properties.get(place))
                    .map(|(_, id, bit)| (*id, *bit)),
                Some(_) | None => cursor.find(name),
            };
            if let Some((id, bit)) = found {
                (met, named) = (met | bit, true);
                let ok = self.fits(id, value) || self.node(id, value, &here, r.part(), 0);
                if !r.took(ok) {
                    return false;
                }
            }
            for (place, id) in &objects.patterns {
                if self.matches(*place, name) {
                    named = true;
                    let ok = self.node(*id, value, &here, r.part(), 0);
                    if !r.took(ok) {
                        return false;
                    }
                }
            }
            let ok = match (named, &objects.extra) {
                (true, _) | (false, Extra::Absent | Extra::Allowed) => true,
                (false, Extra::Checked(id)) => self.node(*id, value, &here, r.part(), 0),
                (false, Extra::Refused) => {
                    r.no(|| said!("{at} has \"{name}\", which the schema does not allow"))
                }
            };
            if !r.took(ok) {
                return false;
            }
        }
        if let Some(bits) = bits
            && met & bits != bits
        {
            return r.no(String::new);
        }
        !objects.closing || self.closing(objects, object, fields, at, r)
    }

    /// Where only `additionalProperties` asks anything of a field: each held to it, nothing
    /// looked up; `None` where other keywords name fields.
    fn only_extra<const F: bool>(
        &self,
        objects: &Objects,
        object: &Value,
        fields: &Map<String, Value>,
        at: &At<'_>,
        r: &mut Report<'_, F>,
    ) -> Option<bool> {
        let Extra::Checked(id) = objects.extra else {
            return None;
        };
        if !objects.properties.is_empty() || !objects.patterns.is_empty() || objects.names.is_some()
        {
            return None;
        }
        for (name, value) in fields {
            let ok = self.fits(id, value) || self.node(id, value, &At::Key(at, name), r.part(), 0);
            if !r.took(ok) {
                return Some(false);
            }
        }
        Some(!objects.closing || self.closing(objects, object, fields, at, r))
    }

    /// What an object is held to once its fields are: the counts, `dependentRequired` and
    /// `dependentSchemas`.
    fn closing<const F: bool>(
        &self,
        objects: &Objects,
        object: &Value,
        fields: &Map<String, Value>,
        at: &At<'_>,
        r: &mut Report<'_, F>,
    ) -> bool {
        if r.full() && !counts(objects, fields, at, r) {
            return false;
        }
        for (name, id) in &objects.dependent_schemas {
            if fields.contains_key(&**name) {
                let ok = self.node(*id, object, at, r.part(), 0);
                if !r.took(ok) {
                    return false;
                }
            }
        }
        true
    }
}

/// `required`, each field looked up: whether to go on.
fn missing<const F: bool>(
    objects: &Objects,
    fields: &Map<String, Value>,
    at: &At<'_>,
    r: &mut Report<'_, F>,
) -> bool {
    for name in &objects.required {
        if !fields.contains_key(&**name) && !r.no(|| said!("{at} is missing \"{name}\"")) {
            return false;
        }
    }
    true
}

/// `minProperties`, `maxProperties` and `dependentRequired`.
fn counts<const F: bool>(
    objects: &Objects,
    fields: &Map<String, Value>,
    at: &At<'_>,
    r: &mut Report<'_, F>,
) -> bool {
    let len = fields.len() as u64;
    if let Some(least) = objects.min_fields.filter(|least| len < *least)
        && !r.no(|| said!("{at} must have at least {least} fields (has {len})"))
    {
        return false;
    }
    if let Some(most) = objects.max_fields.filter(|most| len > *most)
        && !r.no(|| said!("{at} must have at most {most} fields (has {len})"))
    {
        return false;
    }
    for (name, needs) in &objects.dependent {
        if !fields.contains_key(&**name) {
            continue;
        }
        for need in needs.iter().filter(|need| !fields.contains_key(&***need)) {
            if !r.no(|| said!("{at} has \"{name}\", so it must have \"{need}\"")) {
                return false;
            }
        }
    }
    true
}

/// `minItems` and `maxItems`: whether to go on.
fn item_counts<const F: bool>(
    arrays: &Arrays,
    items: &[Value],
    at: &At<'_>,
    r: &mut Report<'_, F>,
) -> bool {
    let len = items.len() as u64;
    if let Some(least) = arrays.min_items.filter(|least| len < *least)
        && !r.no(|| said!("{at} must have at least {least} items (has {len})"))
    {
        return false;
    }
    match arrays.max_items.filter(|most| len > *most) {
        Some(most) => r.no(|| said!("{at} must have at most {most} items (has {len})")),
        None => true,
    }
}
