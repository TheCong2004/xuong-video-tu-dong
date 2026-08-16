import importlib


def load_base_config():
    from config import base_config

    return importlib.reload(base_config)


def test_xhs_international_can_be_enabled_for_rednote_accounts(monkeypatch):
    monkeypatch.setenv("MEDIACRAWLER_XHS_INTERNATIONAL", "true")

    assert load_base_config().XHS_INTERNATIONAL is True


def test_xhs_international_keeps_mainland_default(monkeypatch):
    monkeypatch.delenv("MEDIACRAWLER_XHS_INTERNATIONAL", raising=False)

    assert load_base_config().XHS_INTERNATIONAL is False
