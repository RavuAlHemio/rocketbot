from cirruswikt2germangenders.gender_parsing import parse_complete_gender_spec


def test_m():
    specs = parse_complete_gender_spec("m")
    assert len(specs) == 1
    spec = specs[0]

    assert spec.gender == "m"
    assert not spec.plural_flag
    assert not spec.alternative_genders
    assert not spec.gender_attributes
    assert not spec.usage_attributes
    assert spec.suffixes is None

def test_m_sg():
    specs = parse_complete_gender_spec("m.sg")
    assert len(specs) == 1
    spec = specs[0]

    assert spec.gender == "m"
    assert not spec.plural_flag
    assert not spec.alternative_genders
    assert spec.gender_attributes == ["sg"]
    assert not spec.usage_attributes
    assert spec.suffixes is None

def test_m_weak():
    specs = parse_complete_gender_spec("m.weak")
    assert len(specs) == 1
    spec = specs[0]

    assert spec.gender == "m"
    assert not spec.plural_flag
    assert not spec.alternative_genders
    assert spec.gender_attributes == ["weak"]
    assert not spec.usage_attributes
    assert spec.suffixes is None

def test_m_less_common():
    specs = parse_complete_gender_spec("m.[less common]")
    assert len(specs) == 1
    spec = specs[0]

    assert spec.gender == "m"
    assert not spec.plural_flag
    assert not spec.alternative_genders
    assert not spec.gender_attributes
    assert spec.usage_attributes == ["less common"]
    assert spec.suffixes is None

def test_m_rare():
    specs = parse_complete_gender_spec("m.[rare]")
    assert len(specs) == 1
    spec = specs[0]

    assert spec.gender == "m"
    assert not spec.plural_flag
    assert not spec.alternative_genders
    assert not spec.gender_attributes
    assert spec.usage_attributes == ["rare"]
    assert spec.suffixes is None

def test_m_comma_circum_e():
    specs = parse_complete_gender_spec("m,^e")
    assert len(specs) == 1
    spec = specs[0]

    assert spec.gender == "m"
    assert not spec.plural_flag
    assert not spec.alternative_genders
    assert not spec.gender_attributes
    assert not spec.usage_attributes
    assert spec.suffixes is not None
    assert spec.suffixes.genitive == "^e"
    assert spec.suffixes.plural is None

def test_m_dblcomma_circum_e():
    specs = parse_complete_gender_spec("m,,^e")
    assert len(specs) == 1
    spec = specs[0]

    assert spec.gender == "m"
    assert not spec.plural_flag
    assert not spec.alternative_genders
    assert not spec.gender_attributes
    assert not spec.usage_attributes
    assert spec.suffixes is not None
    assert spec.suffixes.genitive is None
    assert spec.suffixes.plural == "^e"

def test_multispec_goldbugpapagei():
    # example taken from English Wiktionary, entry "Goldbugpapagei"
    specs = parse_complete_gender_spec("((<m,s,en>,<m.weak.[less common]>,<m.[rare]>))")
    assert len(specs) == 3

    assert specs[0].gender == "m"
    assert not specs[0].plural_flag
    assert not specs[0].alternative_genders
    assert not specs[0].gender_attributes
    assert not specs[0].usage_attributes
    assert specs[0].suffixes is not None
    assert specs[0].suffixes.genitive == "s"
    assert specs[0].suffixes.plural == "en"

    assert specs[1].gender == "m"
    assert not specs[1].plural_flag
    assert not specs[1].alternative_genders
    assert specs[1].gender_attributes == ["weak"]
    assert specs[1].usage_attributes == ["less common"]
    assert specs[1].suffixes is None

    assert specs[2].gender == "m"
    assert not specs[2].plural_flag
    assert not specs[2].alternative_genders
    assert not specs[2].gender_attributes
    assert specs[2].usage_attributes == ["rare"]
    assert specs[2].suffixes is None

def test_m_n_rare():
    specs = parse_complete_gender_spec("m:n[rare],s:es,e")
    assert len(specs) == 1
    spec = specs[0]

    assert spec.gender == "m"
    assert not spec.plural_flag
    assert len(spec.alternative_genders) == 1
    assert spec.alternative_genders[0].gender == "n"
    assert not spec.alternative_genders[0].plural_flag
    assert spec.alternative_genders[0].usage_attributes == ["rare"]
    assert not spec.gender_attributes
    assert not spec.usage_attributes
    assert spec.suffixes is not None
    assert spec.suffixes.genitive == "s:es"
    assert spec.suffixes.plural == "e"

def test_f_p():
    specs = parse_complete_gender_spec("f-p")
    assert len(specs) == 1
    spec = specs[0]

    assert spec.gender == "f"
    assert spec.plural_flag
    assert not spec.alternative_genders
    assert not spec.gender_attributes
    assert not spec.usage_attributes
    assert spec.suffixes is None
