from pathlib import Path

from daemon.chunker import MAX_CHUNK, chunks

SAMPLE = Path(__file__).parent / "sample.txt"


def test_short_sentences_pack_under_the_cap():
    assert list(chunks("One. Two! Three?")) == ["One. Two! Three?"]


def test_decimal_not_split():
    assert list(chunks("It cost 3.14 dollars.")) == ["It cost 3.14 dollars."]


def test_abbreviation_packs_with_the_rest():
    assert list(chunks("Dr. Smith arrived.")) == ["Dr. Smith arrived."]


def test_no_trailing_punctuation():
    assert list(chunks("one two three")) == ["one two three"]


def test_whitespace_only():
    assert list(chunks("   \n  ")) == []


def test_empty():
    assert list(chunks("")) == []


def test_missing_space_after_punct_still_splits():
    assert list(chunks("Good.Next sentence.")) == ["Good. Next sentence."]


def test_packing_flushes_when_next_sentence_would_exceed_cap():
    first = "A" * 90 + "."
    second = "B" * 90 + "."
    assert len(first) < MAX_CHUNK
    assert len(first + " " + second) > MAX_CHUNK
    assert list(chunks(first + " " + second)) == [first, second]


def test_long_unpunctuated_splits_on_words():
    word = "abcdefghij"
    words = [word] * 45
    text = " ".join(words)
    parts = list(chunks(text))
    assert len(parts) > 1
    assert all(len(part) <= MAX_CHUNK for part in parts)
    assert " ".join(parts) == text


def test_long_sentence_splits_on_clause_before_words():
    first = ("alpha " * 8).strip()
    second = ("beta " * 40).strip()
    assert len(first) < MAX_CHUNK
    assert len(first + ", " + second) > MAX_CHUNK
    parts = list(chunks(first + ", " + second))
    assert parts[0] == first + ","
    assert all(len(part) <= MAX_CHUNK for part in parts)


def test_word_longer_than_cap_is_sliced():
    extra = 50
    text = "a" * (MAX_CHUNK * 2 + extra)
    assert list(chunks(text)) == ["a" * MAX_CHUNK, "a" * MAX_CHUNK, "a" * extra]


def test_sample_text_packs_and_fixes_missing_spaces():
    parts = list(chunks(SAMPLE.read_text()))
    assert len(parts) > 1
    assert all(len(part) <= MAX_CHUNK for part in parts)
    joined = " ".join(parts)
    assert "numbers. First" in joined
    assert "sentence? Next" in joined
    assert "success. Abbreviations" in joined
    assert "smoothly. Finally" in joined
    assert "she? If" in joined
