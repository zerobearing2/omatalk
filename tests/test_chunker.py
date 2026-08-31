from omatalk.chunker import sentences


def test_basic_split():
    assert sentences("One. Two! Three?") == ["One.", "Two!", "Three?"]


def test_decimal_not_split():
    assert sentences("It cost 3.14 dollars.") == ["It cost 3.14 dollars."]


def test_abbreviation_is_split():
    assert sentences("Dr. Smith arrived.") == ["Dr.", "Smith arrived."]


def test_no_trailing_punctuation():
    assert sentences("one two three") == ["one two three"]


def test_whitespace_only():
    assert sentences("   \n  ") == []


def test_empty():
    assert sentences("") == []
