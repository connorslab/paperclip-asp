"""Laboratory-only two-ledger swap model. No RPC, keys, VTXOs, or real sats.

SQLite transactions model an independently enforced conditional claim on each
server. They do NOT implement Bitcoin Script or prove Ark atomicity. Use only
temporary databases. Amounts represent simulated sats.
"""
import hashlib
import sqlite3


class Ledger:
    def __init__(self, path):
        self.db = sqlite3.connect(path)
        self.db.execute('PRAGMA journal_mode=WAL')
        self.db.executescript('''
            CREATE TABLE IF NOT EXISTS account(owner TEXT PRIMARY KEY, balance INTEGER NOT NULL CHECK(balance>=0));
            CREATE TABLE IF NOT EXISTS leg(id TEXT PRIMARY KEY, payer TEXT NOT NULL,
                payee TEXT NOT NULL, amount INTEGER NOT NULL, hash BLOB NOT NULL,
                deadline INTEGER NOT NULL, state TEXT NOT NULL, secret BLOB);
        ''')

    def seed(self, owner, amount):
        if not isinstance(amount, int) or not 0 <= amount <= 2**63-1:
            raise ValueError('invalid amount')
        with self.db:
            self.db.execute('INSERT INTO account VALUES (?,?)', (owner, amount))

    def balance(self, owner):
        row = self.db.execute('SELECT balance FROM account WHERE owner=?', (owner,)).fetchone()
        return row[0] if row else 0

    def get(self, operation):
        return self.db.execute('SELECT payer,payee,amount,hash,deadline,state,secret FROM leg WHERE id=?',
                               (operation,)).fetchone()

    def lock(self, operation, payer, payee, amount, digest, deadline, height):
        if not 0 < amount < 2**63 or len(digest) != 32 or not 0 <= height < deadline < 500_000_000:
            raise ValueError('invalid contract')
        terms = (payer, payee, amount, digest, deadline)
        with self.db:
            self.db.execute('BEGIN IMMEDIATE')
            old = self.get(operation)
            if old:
                if old[:5] != terms:
                    raise ValueError('operation ID reused with different terms')
                return old[5]
            result = self.db.execute('UPDATE account SET balance=balance-? WHERE owner=? AND balance>=?',
                                     (amount, payer, amount))
            if result.rowcount != 1:
                raise ValueError('insufficient inventory')
            self.db.execute('INSERT INTO leg VALUES (?,?,?,?,?,?,?,NULL)',
                            (operation, *terms, 'locked'))
        return 'locked'

    def claim(self, operation, secret):
        with self.db:
            self.db.execute('BEGIN IMMEDIATE')
            row = self.get(operation)
            if not row or len(secret) != 32 or hashlib.sha256(secret).digest() != row[3]:
                raise ValueError('wrong secret')
            if row[5] == 'claimed':
                return
            if row[5] != 'locked':
                raise ValueError('already refunded')
            # Like an HTLC, success remains available after timeout until refund
            # wins. An application deadline must not pretend this race disappears.
            self._credit(row[1], row[2])
            self.db.execute("UPDATE leg SET state='claimed',secret=? WHERE id=?", (secret, operation))

    def refund(self, operation, height):
        with self.db:
            self.db.execute('BEGIN IMMEDIATE')
            row = self.get(operation)
            if not row or height < row[4]:
                raise ValueError('refund not mature')
            if row[5] == 'refunded':
                return
            if row[5] != 'locked':
                raise ValueError('already claimed')
            self._credit(row[0], row[2])
            self.db.execute("UPDATE leg SET state='refunded' WHERE id=?", (operation,))

    def _credit(self, owner, amount):
        balance = self.balance(owner)
        if balance > 2**63-1-amount:
            raise ValueError('balance overflow')
        self.db.execute('INSERT INTO account VALUES (?,?) ON CONFLICT(owner) DO UPDATE SET balance=excluded.balance',
                        (owner, balance+amount))


class Swap:
    """A coordinator over simulated contracts; restart by reconstructing it.

    `margin` is an assumed lab parameter. Real code must derive it from verified
    recovery graphs. Authentication and quote signatures are not modeled here.
    """
    def __init__(self, source, destination, operation, digest, net, fee, cost,
                 destination_deadline, source_deadline, margin):
        if net <= 0 or cost < 0 or fee < cost or margin <= 0:
            raise ValueError('invalid or subsidized quote')
        if net + fee >= 2**63 or source_deadline-destination_deadline < margin:
            raise ValueError('unsafe quote')
        self.a, self.b, self.id = source, destination, operation
        self.digest, self.net, self.fee = digest, net, fee
        self.tb, self.ta, self.margin = destination_deadline, source_deadline, margin

    def prepare(self, height):
        if height + self.margin >= self.tb:
            raise ValueError('too late to prepare')
        # Reserve provider liquidity first. A crash here leaves a refundable
        # destination reservation, not an unbacked credit to the recipient.
        b = self.b.lock(self.id, 'provider', 'bob', self.net, self.digest, self.tb, height)
        if b != 'locked':
            raise ValueError('destination no longer available')
        a = self.a.lock(self.id, 'alice', 'provider', self.net+self.fee, self.digest, self.ta, height)
        if a != 'locked':
            raise ValueError('source no longer available')

    def receive(self, secret, height):
        a, b = self.a.get(self.id), self.b.get(self.id)
        self._check_terms(a, b)
        if not a or not b or a[5] != 'locked' or b[5] != 'locked':
            raise ValueError('both claims required')
        if height + self.margin >= self.tb:
            raise ValueError('too late to reveal')
        self.b.claim(self.id, secret)
        # A crash between these commits is recovered using the recorded secret.
        self.reconcile()

    def reconcile(self):
        a = self.a.get(self.id)
        b = self.b.get(self.id)
        self._check_terms(a, b)
        if b and b[5] == 'claimed':
            self.a.claim(self.id, b[6])
        a = self.a.get(self.id)
        return bool(a and b and a[5] == b[5] == 'claimed')

    def advance(self, height):
        """Simulated provider monitor. Real claims need chain confirmation."""
        if self.reconcile():
            return 'settled'
        b = self.b.get(self.id)
        if b and b[5] == 'locked' and height >= self.tb:
            self.b.refund(self.id, height)
        b = self.b.get(self.id)
        a = self.a.get(self.id)
        if a and a[5] == 'locked' and b and b[5] == 'refunded' and height >= self.ta:
            self.a.refund(self.id, height)
        a = self.a.get(self.id)
        if b and b[5] == 'refunded' and (not a or a[5] == 'refunded'):
            return 'refunded'
        return 'pending'

    def _check_terms(self, a, b):
        expected_a = ('alice', 'provider', self.net+self.fee, self.digest, self.ta)
        expected_b = ('provider', 'bob', self.net, self.digest, self.tb)
        if (a and a[:5] != expected_a) or (b and b[:5] != expected_b):
            raise ValueError('persisted contract does not match quote')
