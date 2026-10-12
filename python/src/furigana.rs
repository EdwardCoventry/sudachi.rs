use pyo3::exceptions::PyValueError;
use pyo3::prelude::*;
use pyo3::types::{PyDict, PyList};
use sudachi::analysis::furigana::FuriganaInput;

pub fn to_dict<'py>(py: Python<'py>, input: &FuriganaInput) -> PyResult<Bound<'py, PyDict>> {
    let result = PyDict::new(py);
    result.set_item("text", &input.text)?;
    result.set_item("reading", &input.reading)?;
    let spans = PyList::empty(py);
    for span in &input.spans {
        let item = PyDict::new(py);
        item.set_item("surface", &span.surface)?;
        item.set_item("reading", &span.reading)?;
        item.set_item("start", span.start)?;
        item.set_item("end", span.end)?;
        item.set_item("reading_start", span.reading_start)?;
        item.set_item("reading_end", span.reading_end)?;
        item.set_item("annotated", span.annotated)?;
        spans.append(item)?;
    }
    result.set_item("spans", spans)?;
    Ok(result)
}

#[pyfunction]
pub fn parse_furigana<'py>(py: Python<'py>, annotated: &str) -> PyResult<Bound<'py, PyDict>> {
    let input = sudachi::analysis::furigana::parse_furigana(annotated)
        .map_err(|error| PyValueError::new_err(error.to_string()))?;
    to_dict(py, &input)
}
