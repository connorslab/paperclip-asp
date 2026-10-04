"""Coordination tests only; no real Ark contracts or transaction validation."""
import hashlib
import pathlib
import sys
import tempfile
import unittest
from concurrent.futures import ThreadPoolExecutor
from threading import Barrier

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parents[1] / 'scripts'))
from experimental_atomic_swap import Ledger, Swap


class AtomicSwapModel(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.pa, self.pb = [str(pathlib.Path(self.tmp.name) / name) for name in ('a.sqlite', 'b.sqlite')]
        self.a, self.b = Ledger(self.pa), Ledger(self.pb)
        self.a.seed('alice', 100_000)
        self.b.seed('provider', 100_000)
        self.secret = bytes(range(32))
        self.hash = hashlib.sha256(self.secret).digest()
        self.swap = self.coordinator()

    def coordinator(self):
        return Swap(self.a, self.b, 'swap-1', self.hash, 10_000, 200, 200, 200, 240, 20)

    def tearDown(self):
        self.a.db.close()
        self.b.db.close()
        self.tmp.cleanup()

    def test_success_conserves_value_and_retry_does_not_pay_twice(self):
        self.swap.prepare(100)
        self.swap.prepare(100)
        self.swap.receive(self.secret, 110)
        self.assertTrue(self.swap.reconcile())
        self.assertEqual((self.a.balance('alice'), self.a.balance('provider')), (89_800, 10_200))
        self.assertEqual((self.b.balance('provider'), self.b.balance('bob')), (90_000, 10_000))

    def test_crash_after_destination_claim(self):
        self.swap.prepare(100)
        self.b.claim('swap-1', self.secret)
        self.a.db.close(); self.b.db.close()
        self.a, self.b = Ledger(self.pa), Ledger(self.pb)
        self.assertTrue(self.coordinator().reconcile())
        self.assertEqual(self.a.balance('provider'), 10_200)

    def test_timeout_returns_each_leg(self):
        self.swap.prepare(100)
        with self.assertRaises(ValueError): self.a.refund('swap-1', 239)
        self.b.refund('swap-1', 200)
        self.a.refund('swap-1', 240)
        self.a.refund('swap-1', 250)
        self.assertEqual(self.a.balance('alice'), 100_000)
        self.assertEqual(self.b.balance('provider'), 100_000)
        self.assertFalse(self.swap.reconcile())

    def test_insufficient_destination_inventory_does_not_lock_source(self):
        self.b.db.execute("UPDATE account SET balance=0 WHERE owner='provider'")
        self.b.db.commit()
        with self.assertRaises(ValueError): self.swap.prepare(100)
        self.assertIsNone(self.a.get('swap-1'))

    def test_partial_preparation_can_refund(self):
        self.a.db.execute("UPDATE account SET balance=0 WHERE owner='alice'")
        self.a.db.commit()
        with self.assertRaises(ValueError): self.swap.prepare(100)
        self.b.refund('swap-1', 200)
        self.assertEqual(self.b.balance('provider'), 100_000)

    def test_wrong_secret_and_conflicting_retry(self):
        self.swap.prepare(100)
        with self.assertRaises(ValueError): self.swap.receive(bytes(32), 110)
        with self.assertRaises(ValueError):
            self.a.lock('swap-1', 'alice', 'attacker', 10_200, self.hash, 240, 100)
        self.assertEqual(self.b.balance('bob'), 0)

    def test_expired_or_underpriced_quote(self):
        with self.assertRaises(ValueError): self.swap.prepare(180)
        with self.assertRaises(ValueError):
            Swap(self.a, self.b, 'x', self.hash, 10_000, 10, 200, 200, 240, 20)
        with self.assertRaises(ValueError):
            Swap(self.a, self.b, 'x', self.hash, 10_000, 200, 200, 200, 210, 20)

    def test_late_disclosure_exposes_monitoring_assumption(self):
        self.swap.prepare(100)
        with self.assertRaises(ValueError): self.swap.receive(self.secret, 200)
        # The contract model correctly allows a success/refund race. If the
        # provider misses its source window, atomic recovery is NOT guaranteed.
        self.a.refund('swap-1', 240)
        self.b.claim('swap-1', self.secret)
        with self.assertRaises(ValueError): self.swap.reconcile()
        self.assertEqual(self.b.balance('bob'), 10_000)

    def test_monitor_refunds_destination_before_source(self):
        self.swap.prepare(100)
        self.assertEqual(self.swap.advance(200), 'pending')
        with self.assertRaises(ValueError): self.b.claim('swap-1', self.secret)
        self.assertEqual(self.swap.advance(240), 'refunded')
        self.assertEqual(self.a.balance('alice'), 100_000)
        self.assertEqual(self.b.balance('provider'), 100_000)

    def test_restart_with_changed_quote_cannot_settle(self):
        self.swap.prepare(100)
        changed = Swap(self.a, self.b, 'swap-1', self.hash, 9000, 200, 200, 200, 240, 20)
        with self.assertRaises(ValueError): changed.receive(self.secret, 110)
        with self.assertRaises(ValueError): changed.reconcile()
        self.assertEqual(self.b.balance('bob'), 0)

    def race(self, first, second):
        barrier = Barrier(2)
        def run(action):
            ledger = Ledger(self.pb)
            try:
                barrier.wait(timeout=5)
                try:
                    action(ledger)
                    return True
                except ValueError:
                    return False
            finally:
                ledger.db.close()
        with ThreadPoolExecutor(max_workers=2) as pool:
            a, b = pool.submit(run, first), pool.submit(run, second)
            return a.result(timeout=10), b.result(timeout=10)

    def test_competing_claim_and_refund_credit_exactly_one_owner(self):
        self.swap.prepare(100)
        results = self.race(lambda l: l.claim('swap-1', self.secret),
                            lambda l: l.refund('swap-1', 200))
        self.assertEqual(sum(results), 1)
        self.assertEqual(self.b.balance('provider')+self.b.balance('bob'), 100_000)
        self.assertIn(self.b.get('swap-1')[5], ('claimed', 'refunded'))

    def test_duplicate_concurrent_claim_is_idempotent(self):
        self.swap.prepare(100)
        self.assertEqual(self.race(lambda l: l.claim('swap-1', self.secret),
                                   lambda l: l.claim('swap-1', self.secret)), (True, True))
        self.assertEqual(self.b.balance('bob'), 10_000)

    def test_two_orders_cannot_reserve_same_inventory(self):
        results = self.race(
            lambda l: l.lock('one', 'provider', 'bob', 80_000, self.hash, 200, 100),
            lambda l: l.lock('two', 'provider', 'bob', 80_000, self.hash, 200, 100))
        self.assertEqual(sum(results), 1)
        self.assertEqual(self.b.balance('provider'), 20_000)
        self.assertEqual(self.b.db.execute("SELECT sum(amount) FROM leg WHERE state='locked'").fetchone()[0], 80_000)


if __name__ == '__main__':
    unittest.main()
