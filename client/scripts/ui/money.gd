extends RefCounted
## Money formatting for the inspector UI. The sim stores money as integer pence (spec §3,
## docs/inspector-protocol.md); the client only ever formats it, never does sums in floats.
##
## `£1,234` for whole pounds, `£1,234.56` when there are odd pence, `−£50` for negatives
## (a true minus sign, U+2212). Works on the decimal string, so even the i64 extremes format.

const MINUS := "−"


## Formats pence as pounds: `£1,234`, `£0.50`, `−£50`.
static func format_pence(p: int) -> String:
	var digits := str(p)
	var negative := digits.begins_with("-")
	digits = digits.trim_prefix("-").lpad(3, "0")
	var pounds := _group_thousands(digits.left(-2))
	var pence := digits.right(2)
	var text := "£" + pounds
	if pence != "00":
		text += "." + pence
	return (MINUS + text) if negative else text


## Like `format_pence`, with a `+` on gains, for profit and change figures.
static func format_signed(p: int) -> String:
	return ("+" + format_pence(p)) if p > 0 else format_pence(p)


## "1234567" -> "1,234,567".
static func _group_thousands(digits: String) -> String:
	var out := ""
	var count := 0
	for i in range(digits.length() - 1, -1, -1):
		if count > 0 and count % 3 == 0:
			out = "," + out
		out = digits[i] + out
		count += 1
	return out
