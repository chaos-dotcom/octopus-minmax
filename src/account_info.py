from tariff import Tariff


class AccountInfo:
    def __init__(self, current_tariff: Tariff, standing_charge: float, region_code: str, consumption,
                 mpan: str, product_code: str = None):
        self.current_tariff = current_tariff
        self.standing_charge = standing_charge
        self.region_code = region_code
        self.consumption = consumption
        self.mpan = mpan
        # The Octopus product code for the *current* tariff (e.g. "COSY-FIX-12M-26-06-25").
        # Used to fetch the current tariff's own rates when the consumption source
        # does not provide cost deltas (e.g. Home Assistant).
        self.product_code = product_code
