extends GutTest
## Money formatting for the inspector UI: integer pence in, pounds out (spec §3).

const Money := preload("res://scripts/ui/money.gd")


func test_zero() -> void:
	assert_eq(Money.format_pence(0), "£0")


func test_whole_pounds_drop_the_pence() -> void:
	assert_eq(Money.format_pence(100), "£1")
	assert_eq(Money.format_pence(5000), "£50")


func test_thousands_are_grouped() -> void:
	assert_eq(Money.format_pence(123_400), "£1,234")
	assert_eq(Money.format_pence(700_000), "£7,000")
	assert_eq(Money.format_pence(99_999_900), "£999,999")


func test_millions() -> void:
	assert_eq(Money.format_pence(100_000_000), "£1,000,000")
	assert_eq(Money.format_pence(12_345_678_900), "£123,456,789")


func test_odd_pence_show_two_decimals() -> void:
	assert_eq(Money.format_pence(1), "£0.01")
	assert_eq(Money.format_pence(50), "£0.50")
	assert_eq(Money.format_pence(123_456), "£1,234.56")


func test_negatives_use_a_true_minus_before_the_pound_sign() -> void:
	assert_eq(Money.format_pence(-5000), "−£50")
	assert_eq(Money.format_pence(-1), "−£0.01")
	assert_eq(Money.format_pence(-123_456_700), "−£1,234,567")


func test_extremes_do_not_overflow() -> void:
	assert_eq(Money.format_pence(9223372036854775807), "£92,233,720,368,547,758.07")
	assert_eq(Money.format_pence(-9223372036854775807 - 1), "−£92,233,720,368,547,758.08")


func test_signed_shows_plus_for_gains() -> void:
	assert_eq(Money.format_signed(2500), "+£25")
	assert_eq(Money.format_signed(-2500), "−£25")
	assert_eq(Money.format_signed(0), "£0")
