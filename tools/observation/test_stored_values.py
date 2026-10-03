"""Exact storage patterns without a debugger or a game."""
import sys
from pathlib import Path
import unittest
from unittest.mock import Mock

SOURCE = Path(__file__).resolve().parents[2] / 'src/binding/platform/macos/observation'
sys.path.insert(0, str(SOURCE))
import stored_values
import protocol


TAG = 23


def short_cstring(text):
    encoded = text.encode('utf-8')
    return encoded.ljust(TAG, b'\0') + bytes([len(encoded)])


def long_cstring(pointer, length):
    return (pointer.to_bytes(8, 'little') + length.to_bytes(8, 'little')).ljust(TAG, b'\0') + bytes([128])


class CStringTests(unittest.TestCase):
    def test_short_text_is_in_place_and_multibyte_text_decodes(self):
        read = Mock(side_effect=AssertionError('heap read'))
        for text in ['', 'pop_happiness', 'Zürich ✓']:
            with self.subTest(text=text):
                self.assertEqual(stored_values.cstring(short_cstring(text), TAG, read), text)

    def test_a_short_tag_beyond_its_storage_is_malformed(self):
        storage = short_cstring('x')[:TAG] + bytes([TAG + 1])
        with self.assertRaisesRegex(RuntimeError, 'short string length outside bound'):
            stored_values.cstring(storage, TAG, Mock())

    def test_long_text_reads_its_length_behind_the_pointer(self):
        text = 'é' * 2048
        read = Mock(return_value=text.encode('utf-8'))
        self.assertEqual(stored_values.cstring(long_cstring(0x2000, 4096), TAG, read), text)
        read.assert_called_once_with(0x2000, 4096)

    def test_a_long_length_beyond_the_bound_is_not_read(self):
        read = Mock()
        with self.assertRaisesRegex(RuntimeError, 'string length outside bound'):
            stored_values.cstring(long_cstring(0x2000, 4097), TAG, read)
        read.assert_not_called()

    def test_a_truncated_read_raises(self):
        read = Mock(side_effect=RuntimeError('native memory access failed'))
        with self.assertRaisesRegex(RuntimeError, 'native memory access failed'):
            stored_values.cstring(long_cstring(0x2000, 40), TAG, read)
        cut = 'Zürich'.encode('utf-8')[:2]
        with self.assertRaises(UnicodeDecodeError):
            stored_values.cstring(long_cstring(0x2000, 2), TAG, Mock(return_value=cut))


class StoredValueTests(unittest.TestCase):
    def test_float_preserves_representative_binary32_patterns(self):
        for bits in [0, 0x80000000, 0x3f9e0652, 0x7f7fffff, 0x7f800000, 0xff800000, 0x7fc00001]:
            with self.subTest(bits=bits):
                read = Mock(return_value=bits)
                text = Mock(side_effect=AssertionError('string read'))
                value = stored_values.decode(read, text, 0x1000, 'Float')
                self.assertEqual(value, {'Float': dict(bits=bits)})
                read.assert_called_once_with(0x1000, 4)
                root = protocol.SCHEMAS['record']
                protocol.validate(value, root['$defs']['FixtureValue'], root)

    def test_short_preserves_bits_without_a_sign(self):
        for bits in [0, 32767, 32768, 65535]:
            with self.subTest(bits=bits):
                read = Mock(return_value=bits)
                value = stored_values.decode(read, Mock(), 0x1000, 'Integer16')
                self.assertEqual(value, {'Integer16': dict(bits=bits)})
                read.assert_called_once_with(0x1000, 2)
                root = protocol.SCHEMAS['record']
                protocol.validate(value, root['$defs']['FixtureValue'], root)

    def test_float_and_short_do_not_invent_values_on_failed_memory_reads(self):
        for decoder in ['Float', 'Integer16']:
            with self.subTest(decoder=decoder), self.assertRaisesRegex(RuntimeError, 'missing storage'):
                stored_values.decode(Mock(side_effect=RuntimeError('missing storage')), Mock(), 0x1000, decoder)

    def test_missing_decoder_is_unavailable(self):
        read = Mock()
        with self.assertRaisesRegex(RuntimeError, 'decoder is unavailable'):
            stored_values.decode(read, Mock(), 0x1000, None)
        read.assert_not_called()

    def test_codec_rejects_missing_and_out_of_width_bits(self):
        root = protocol.SCHEMAS['record']
        schema = root['$defs']['FixtureValue']
        for variant, width in [('Float', 32), ('Integer16', 16)]:
            for value in [{variant: {}}, {variant: dict(bits=-1)}, {variant: dict(bits=1 << width)}]:
                with self.subTest(value=value), self.assertRaises(ValueError):
                    protocol.validate(value, schema, root)
