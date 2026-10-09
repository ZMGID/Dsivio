import unittest

from select_candidates import select, sales_value


class CandidateTests(unittest.TestCase):
    def test_abbreviated_sales_participate_and_original_is_preserved(self):
        rows = [dict(id=str(i), price=p, salesRaw=s) for i, (p, s) in enumerate(
            [(35.77, '853'), (12, '52'), (10.49, '40mil+'), (20, '100mil+'), (8, '0')])]
        result = select(rows)
        self.assertEqual([r['id'] for r in result], ['3', '0', '2'])
        self.assertEqual(result[0]['salesRaw'], '100mil+')
        self.assertEqual(result[0]['roles'], ['销量最高'])

    def test_localized_units_missing_data_and_duplicate_roles(self):
        for raw, value in [('1,5mil+', 1500), ('1.5k+', 1500), ('10万+', 100000),
                           ('1.234', 1234), ('1,234', 1234), ('1.234 vendidos', 1234)]:
            self.assertEqual(sales_value(raw), value)
        self.assertIsNone(sales_value('未披露'))
        result = select([dict(id='x', price=1, salesRaw='5'),
                         dict(id='x', price=1, salesRaw='5'),
                         dict(id='y', price=0, salesRaw='100'),
                         dict(id='z', price=2, salesRaw='9', included=False)])
        self.assertEqual(len(result), 1)
        self.assertEqual(len(result[0]['roles']), 3)

    def test_mixed_currencies_rejected(self):
        with self.assertRaises(ValueError):
            select([dict(id='a', price=1, salesRaw='8', currency='BRL'),
                    dict(id='b', price=2, salesRaw='9', currency='USD')])

    def test_mixed_periods_rejected(self):
        with self.assertRaises(ValueError):
            select([dict(id='a', price=1, salesRaw='8', period='累计'),
                    dict(id='b', price=2, salesRaw='9', period='近30天')])


if __name__ == '__main__':
    unittest.main()
