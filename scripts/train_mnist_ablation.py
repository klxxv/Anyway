"""Run a small, reproducible MNIST ablation study for the Research Canvas demo.

The script intentionally trains on a stratified subset because the host used for
the MVP has constrained available memory and no GPU. Results are written as a
stable JSON artifact consumed by the bundled Git experiment plugin.
"""

from __future__ import annotations

import gc
import json
import os
import time
import urllib.request
import warnings
from pathlib import Path

import numpy as np
from sklearn.exceptions import ConvergenceWarning
from sklearn.metrics import accuracy_score, log_loss
from sklearn.model_selection import train_test_split
from sklearn.neural_network import MLPClassifier

ROOT = Path(__file__).resolve().parents[1]
WORK_DIR = ROOT / "work" / "mnist"
DATA_PATH = WORK_DIR / "mnist.npz"
OUTPUT_PATH = ROOT / "app" / "data" / "mnist-experiment-results.json"
DATA_URL = "https://storage.googleapis.com/tensorflow/tf-keras-datasets/mnist.npz"
RANDOM_STATE = 42
TRAIN_SAMPLES = 6_000
TEST_SAMPLES = 1_500


def ensure_dataset() -> None:
    WORK_DIR.mkdir(parents=True, exist_ok=True)
    if DATA_PATH.exists():
        return
    print(f"Downloading MNIST to {DATA_PATH}...")
    urllib.request.urlretrieve(DATA_URL, DATA_PATH)


def load_subset() -> tuple[np.ndarray, np.ndarray, np.ndarray, np.ndarray]:
    with np.load(DATA_PATH) as data:
        images = np.concatenate((data["x_train"], data["x_test"]), axis=0)
        labels = np.concatenate((data["y_train"], data["y_test"]), axis=0)

    selected_images, _, selected_labels, _ = train_test_split(
        images,
        labels,
        train_size=TRAIN_SAMPLES + TEST_SAMPLES,
        stratify=labels,
        random_state=RANDOM_STATE,
    )
    x_train, x_test, y_train, y_test = train_test_split(
        selected_images,
        selected_labels,
        train_size=TRAIN_SAMPLES,
        test_size=TEST_SAMPLES,
        stratify=selected_labels,
        random_state=RANDOM_STATE,
    )
    del images, labels, selected_images, selected_labels
    gc.collect()
    return x_train.reshape(TRAIN_SAMPLES, -1), x_test.reshape(TEST_SAMPLES, -1), y_train, y_test


def train_variant(
    *,
    experiment_id: str,
    label: str,
    hypothesis: str,
    x_train_u8: np.ndarray,
    x_test_u8: np.ndarray,
    y_train: np.ndarray,
    y_test: np.ndarray,
    normalized: bool,
    hidden_units: int,
    activation: str,
) -> dict[str, object]:
    if normalized:
        x_train = x_train_u8.astype(np.float32) / 255.0
        x_test = x_test_u8.astype(np.float32) / 255.0
    else:
        x_train = x_train_u8.astype(np.float32)
        x_test = x_test_u8.astype(np.float32)

    model = MLPClassifier(
        hidden_layer_sizes=(hidden_units,),
        activation=activation,
        solver="adam",
        alpha=1e-4,
        batch_size=128,
        learning_rate_init=1e-3,
        max_iter=12,
        early_stopping=True,
        validation_fraction=0.12,
        n_iter_no_change=3,
        random_state=RANDOM_STATE,
        verbose=False,
    )
    started = time.perf_counter()
    with warnings.catch_warnings():
        warnings.filterwarnings("ignore", category=ConvergenceWarning)
        model.fit(x_train, y_train)
    duration = time.perf_counter() - started
    probabilities = model.predict_proba(x_test)
    predictions = probabilities.argmax(axis=1)
    accuracy = accuracy_score(y_test, predictions)
    loss = log_loss(y_test, probabilities, labels=np.arange(10))

    result = {
        "id": experiment_id,
        "label": label,
        "hypothesis": hypothesis,
        "normalized": normalized,
        "hiddenUnits": hidden_units,
        "activation": activation,
        "accuracy": round(float(accuracy), 4),
        "logLoss": round(float(loss), 4),
        "iterations": int(model.n_iter_),
        "durationSeconds": round(float(duration), 2),
        "finalTrainingLoss": round(float(model.loss_), 5),
    }
    del model, x_train, x_test, probabilities, predictions
    gc.collect()
    return result


def main() -> None:
    os.environ.setdefault("OMP_NUM_THREADS", "4")
    os.environ.setdefault("OPENBLAS_NUM_THREADS", "4")
    os.environ.setdefault("MKL_NUM_THREADS", "4")
    ensure_dataset()
    x_train, x_test, y_train, y_test = load_subset()

    variants = [
        {
            "experiment_id": "mnist-baseline",
            "label": "Baseline · 64 ReLU units",
            "hypothesis": "A normalized 64-unit hidden representation is a sufficient baseline.",
            "normalized": True,
            "hidden_units": 64,
            "activation": "relu",
        },
        {
            "experiment_id": "mnist-no-normalization",
            "label": "Ablate pixel normalization",
            "hypothesis": "Pixel normalization is required for stable optimization.",
            "normalized": False,
            "hidden_units": 64,
            "activation": "relu",
        },
        {
            "experiment_id": "mnist-bottleneck-16",
            "label": "Reduce hidden width to 16",
            "hypothesis": "A 16-unit bottleneck materially reduces test accuracy.",
            "normalized": True,
            "hidden_units": 16,
            "activation": "relu",
        },
        {
            "experiment_id": "mnist-tanh",
            "label": "Replace ReLU with tanh",
            "hypothesis": "ReLU is uniquely necessary for this small MNIST model.",
            "normalized": True,
            "hidden_units": 64,
            "activation": "tanh",
        },
    ]

    results: list[dict[str, object]] = []
    for index, variant in enumerate(variants, start=1):
        print(f"[{index}/{len(variants)}] {variant['label']}")
        result = train_variant(
            **variant,
            x_train_u8=x_train,
            x_test_u8=x_test,
            y_train=y_train,
            y_test=y_test,
        )
        print(
            f"  accuracy={result['accuracy']:.4f} "
            f"log_loss={result['logLoss']:.4f} "
            f"duration={result['durationSeconds']:.2f}s"
        )
        results.append(result)

    baseline_accuracy = float(results[0]["accuracy"])
    for result in results:
        delta = float(result["accuracy"]) - baseline_accuracy
        result["deltaAccuracy"] = round(delta, 4)
        if result["id"] == "mnist-baseline":
            result["evidenceOutcome"] = "baseline"
        elif result["id"] == "mnist-tanh":
            # A small drop does not establish that ReLU is uniquely necessary.
            result["evidenceOutcome"] = "refutes" if delta >= -0.015 else "supports"
        else:
            result["evidenceOutcome"] = "supports" if delta <= -0.01 else "refutes"

    artifact = {
        "schemaVersion": 1,
        "task": "MNIST small-MLP ablation",
        "dataset": {
            "name": "MNIST",
            "source": DATA_URL,
            "trainSamples": TRAIN_SAMPLES,
            "testSamples": TEST_SAMPLES,
            "inputShape": [28, 28],
            "classes": 10,
        },
        "environment": {
            "runtime": "scikit-learn MLPClassifier",
            "device": "CPU",
            "randomState": RANDOM_STATE,
            "gitCommit": "b7e21ac",
            "repository": "research-canvas/mnist-ablation-demo",
        },
        "results": results,
    }
    OUTPUT_PATH.parent.mkdir(parents=True, exist_ok=True)
    OUTPUT_PATH.write_text(json.dumps(artifact, indent=2), encoding="utf-8")
    print(f"Wrote {OUTPUT_PATH}")


if __name__ == "__main__":
    main()
