"""Tests of Discord messages from scripts: multialt.discord and the notify block."""

from __future__ import annotations

import unittest

import multialt
from multialt import Account
from multialt._core.protocol import MultiAltError
from multialt.defaults import notify
from tests.fake_transport import FakeTransport


class DiscordTests(unittest.TestCase):
    def setUp(self) -> None:
        self.fake = FakeTransport().install()
        notify._last_sent.clear()

    def tearDown(self) -> None:
        self.fake.uninstall()

    def sent(self) -> list[dict]:
        return [args for op, args in self.fake.calls if op == "discord.send"]

    def test_discord_sends_the_text_through_the_app(self) -> None:
        multialt.discord("Farm done", title="Night run")
        self.assertEqual(self.sent(), [{"text": "Farm done", "title": "Night run"}])

    def test_an_account_message_carries_the_account(self) -> None:
        Account(user_id=77, name="Alt").discord("Egg hatched")
        self.assertEqual(self.sent(), [{"text": "Egg hatched", "user_id": 77}])

    def test_a_refusal_from_the_app_is_raised(self) -> None:
        def refuse(_args: dict) -> None:
            raise MultiAltError("Discord alerts are off.", "Turn them on in Discord alerts.", code="webhook_off")

        self.fake.handlers["discord.send"] = refuse
        with self.assertRaises(MultiAltError) as caught:
            multialt.discord("hi")
        self.assertEqual(caught.exception.code, "webhook_off")

    def test_send_report_lists_each_value_on_its_own_line(self) -> None:
        notify.send_report({"Loops": 12, "Coins": "3.4k"}, title="Farm report")
        self.assertEqual(self.sent(), [{"text": "**Loops:** 12\n**Coins:** 3.4k", "title": "Farm report"}])

    def test_send_at_most_every_skips_messages_that_come_too_soon(self) -> None:
        self.assertTrue(notify.send_at_most_every(10, "first", key="loop"))
        self.assertFalse(notify.send_at_most_every(10, "second", key="loop"))
        # Outra chave não espera a primeira.
        self.assertTrue(notify.send_at_most_every(10, "other", key="egg"))
        self.assertEqual([a["text"] for a in self.sent()], ["first", "other"])

    def test_notify_errors_reports_the_error_and_raises_it_again(self) -> None:
        with self.assertRaises(ValueError):
            with notify.notify_errors(title="Script failed"):
                raise ValueError("no coins")
        self.assertEqual(self.sent(), [{"text": "ValueError: no coins", "title": "Script failed"}])

    def test_notify_errors_does_not_report_a_stop(self) -> None:
        with self.assertRaises(multialt.Stopped):
            with notify.notify_errors():
                raise multialt.Stopped("stop")
        self.assertEqual(self.sent(), [])


if __name__ == "__main__":
    unittest.main()
