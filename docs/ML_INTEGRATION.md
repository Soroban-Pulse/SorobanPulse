# AI/ML Integration & Intelligence Features

**Issue #842**

This document describes the AI/ML integration capabilities for intelligent features in SorobanPulse.

## Overview

The ML integration module provides:

- **Advanced Anomaly Detection**: ML-enhanced forecasting and detection
- **Pattern Recognition**: Automatic discovery of event patterns
- **Event Classification**: Intelligent event categorization
- **Predictive Forecasting**: Time series predictions with confidence intervals
- **Intelligent Filtering**: Auto-learned filters based on usage patterns
- **Optimization Recommendations**: ML-driven suggestions for configuration

## Features

### 1. ML-Enhanced Anomaly Detection

Improves upon basic statistical methods with machine learning.

#### Holt-Winters Triple Exponential Smoothing

Captures level, trend, and seasonality in time series data:

```rust
use soroban_pulse::ml_integration::MLEnhancedForecaster;

// Create forecaster with 24-hour seasonality
let mut forecaster = MLEnhancedForecaster::new(24, 3.0);

// Train with historical data
for value in historical_values {
    forecaster.update(value);
}

// Forecast next 7 steps
let predictions = forecaster.forecast(7);

// Check if new value is anomalous
if forecaster.is_anomalous(new_value) {
    alert("Anomaly detected!");
}
```

#### Features

- **Level Smoothing**: Tracks the baseline value
- **Trend Detection**: Identifies upward or downward trends
- **Seasonal Patterns**: Learns daily, weekly, or custom cycles
- **Prediction Intervals**: Confidence bounds around forecasts
- **Adaptive Learning**: Continuously updates with new data

### 2. Pattern Recognition

Automatically discovers recurring patterns in event sequences.

#### Dynamic Time Warping (DTW)

Compares event sequences to detect similar patterns:

```rust
use soroban_pulse::ml_integration::PatternRecognitionEngine;

let mut engine = PatternRecognitionEngine::new(0.7); // 70% confidence threshold

// Learn patterns
engine.learn_pattern("normal_flow", vec![1.0, 2.0, 3.0, 4.0]);
engine.learn_pattern("attack_pattern", vec![10.0, 50.0, 100.0, 200.0]);

// Detect patterns in new sequence
let detected = engine.detect_patterns(&new_sequence);
for (pattern_name, confidence) in detected {
    println!("Detected {} with {}% confidence", pattern_name, confidence * 100.0);
}
```

#### Use Cases

- **Fraud Detection**: Identify suspicious transaction patterns
- **Performance Issues**: Detect degraded performance signatures
- **Attack Patterns**: Recognize known attack sequences
- **Business Events**: Track multi-step business processes

### 3. Event Classification

Categorize events using Gaussian Naive Bayes classifier.

```rust
use soroban_pulse::ml_integration::EventClassifier;

let mut classifier = EventClassifier::new();

// Train with labeled examples
let mut features = HashMap::new();
features.insert("value", 100.0);
features.insert("latency", 50.0);
features.insert("size", 1024.0);
classifier.train("normal", features);

// More training examples...

// Classify new event
let mut test_features = HashMap::new();
test_features.insert("value", 105.0);
test_features.insert("latency", 55.0);
test_features.insert("size", 1100.0);

if let Some((class, confidence)) = classifier.classify(&test_features) {
    println!("Classified as {} with {}% confidence", class, confidence * 100.0);
}
```

#### Supported Features

- **Numerical Features**: Value, latency, size, count, etc.
- **Derived Features**: Ratios, deltas, moving averages
- **Temporal Features**: Hour of day, day of week, etc.
- **Contextual Features**: Contract ID, event type, etc.

### 4. Predictive Forecasting

Time series forecasting with confidence intervals.

```rust
use soroban_pulse::ml_integration::train_anomaly_model;

// Train model on historical data
let model = train_anomaly_model(
    &pool,
    "CABC...",  // contract_id
    30          // lookback days
).await?;

println!("Model accuracy: {:.2}%", model.accuracy.unwrap() * 100.0);
println!("Training samples: {}", model.training_samples);
```

#### Model Metrics

- **Accuracy**: Overall correctness of predictions
- **Precision**: True positives / (True positives + False positives)
- **Recall**: True positives / (True positives + False negatives)
- **F1 Score**: Harmonic mean of precision and recall

### 5. Intelligent Filtering

Auto-learned filters that adapt based on user behavior.

```rust
use soroban_pulse::ml_integration::{create_intelligent_filter, FilterAction};

let conditions = vec![
    FilterCondition {
        field: "event_type".to_string(),
        operator: "equals".to_string(),
        value: json!("contract"),
        weight: 0.8,
    },
    FilterCondition {
        field: "value".to_string(),
        operator: "greater_than".to_string(),
        value: json!(1000),
        weight: 0.6,
    },
];

let filter = create_intelligent_filter(
    &pool,
    "High-value contract events".to_string(),
    conditions,
    FilterAction::Priority("high".to_string())
).await?;
```

#### Filter Actions

- **Route**: Send to specific channel
- **Tag**: Add metadata tags
- **Priority**: Set event priority
- **Alert**: Trigger immediate notification
- **Suppress**: Filter out noise

#### Performance Tracking

Filters track their own performance:

```json
{
  "true_positives": 450,
  "false_positives": 12,
  "true_negatives": 8930,
  "false_negatives": 8,
  "precision": 0.974,
  "recall": 0.982,
  "f1_score": 0.978
}
```

### 6. Optimization Recommendations

ML-driven suggestions for improving configuration.

```rust
use soroban_pulse::ml_integration::get_optimization_recommendations;

let recommendations = get_optimization_recommendations(&pool, tenant_id).await?;

for rec in recommendations {
    println!("💡 {}", rec);
}
```

**Example Recommendations:**

- "Consider consolidating subscriptions with similar filters to reduce overhead"
- "High event volume detected. Consider using intelligent filters to reduce noise"
- "Pattern detected: 85% of events occur between 9 AM - 5 PM. Adjust webhook delivery schedule"
- "Anomaly detection threshold can be tightened based on recent stability"

## Configuration

### Environment Variables

```bash
# Enable ML features
ML_ANOMALY_DETECTION_ENABLED=true
ML_PATTERN_RECOGNITION_ENABLED=true
ML_INTELLIGENT_FILTERING_ENABLED=true

# Model training configuration
ML_MODEL_TRAINING_INTERVAL_HOURS=24
ML_MIN_TRAINING_SAMPLES=100
ML_CONFIDENCE_THRESHOLD=0.8
ML_AUTO_RETRAIN=true
```

### Database Setup

Run the migration to create ML tables:

```bash
psql $DATABASE_URL < migrations/20250826_saas_ml_features.sql
```

## API Endpoints

### Model Management

#### POST `/v1/ml/models/train`

Train a new ML model.

**Request:**
```json
{
  "model_type": "anomaly_detection",
  "contract_id": "CABC123...",
  "lookback_days": 30
}
```

**Response:**
```json
{
  "model_id": "550e8400-e29b-41d4-a716-446655440000",
  "name": "anomaly_model_CABC123",
  "accuracy": 0.94,
  "training_samples": 5000,
  "last_trained_at": "2026-08-26T12:00:00Z"
}
```

#### GET `/v1/ml/models`

List all ML models.

#### GET `/v1/ml/models/:model_id`

Get model details including performance metrics.

### Pattern Detection

#### POST `/v1/ml/patterns/detect`

Detect patterns in recent events.

**Request:**
```json
{
  "contract_id": "CABC123...",
  "lookback_hours": 24
}
```

**Response:**
```json
{
  "patterns": [
    {
      "pattern_id": "660e8400-e29b-41d4-a716-446655440000",
      "pattern_type": "sequence",
      "confidence": 0.85,
      "description": "Recurring event sequence: transfer->mint->burn",
      "frequency": 127,
      "first_seen": "2026-08-25T00:00:00Z",
      "last_seen": "2026-08-26T12:00:00Z"
    }
  ]
}
```

### Predictions

#### POST `/v1/ml/predict`

Get ML predictions for future values.

**Request:**
```json
{
  "model_id": "550e8400-e29b-41d4-a716-446655440000",
  "steps": 7,
  "features": {
    "hour_of_day": 14,
    "day_of_week": 3
  }
}
```

**Response:**
```json
{
  "predictions": [
    {
      "step": 1,
      "predicted_value": 125.5,
      "confidence_interval": [118.2, 132.8],
      "confidence_score": 0.92
    },
    ...
  ]
}
```

### Intelligent Filters

#### GET `/v1/ml/filters`

List intelligent filters.

#### POST `/v1/ml/filters`

Create a new intelligent filter.

#### GET `/v1/ml/filters/:filter_id/performance`

Get filter performance metrics.

### Recommendations

#### GET `/v1/ml/recommendations`

Get optimization recommendations for a tenant.

**Response:**
```json
{
  "recommendations": [
    "Consider consolidating subscriptions with similar filters",
    "High event volume detected. Use intelligent filters to reduce noise",
    "Pattern detected: Most events occur 9 AM - 5 PM"
  ]
}
```

## Use Cases

### 1. Fraud Detection

```rust
// Train classifier on known fraud patterns
let mut classifier = EventClassifier::new();

// Load historical labeled data
for (features, label) in training_data {
    classifier.train(label, features);
}

// Classify incoming transactions
for transaction in incoming_transactions {
    let features = extract_features(&transaction);
    if let Some(("fraud", confidence)) = classifier.classify(&features) {
        if confidence > 0.9 {
            block_transaction(&transaction);
            alert_security_team(&transaction);
        }
    }
}
```

### 2. Capacity Planning

```rust
// Forecast future load
let mut forecaster = MLEnhancedForecaster::new(168, 3.0); // Weekly seasonality

// Train on historical load data
for load in historical_loads {
    forecaster.update(load);
}

// Forecast next week
let forecast = forecaster.forecast(168); // 168 hours = 1 week

// Check if capacity upgrade needed
if forecast.iter().any(|&v| v > capacity_threshold) {
    recommend_capacity_upgrade();
}
```

### 3. Anomaly Alerting

```rust
// Configure anomaly detection
let model = train_anomaly_model(&pool, contract_id, 30).await?;

// Monitor for anomalies
for event in event_stream {
    let value = extract_metric(&event);
    
    if forecaster.is_anomalous(value) {
        let alert = AnomalyAlert {
            metric_name: "transaction_volume",
            metric_value: value,
            expected_range: forecaster.prediction_interval(1, 3.0),
            anomaly_score: calculate_score(value, &forecaster),
            severity: determine_severity(value, &forecaster),
        };
        
        send_alert(&alert).await?;
    }
}
```

### 4. Intelligent Routing

```rust
// Auto-learn routing rules
let mut patterns = HashMap::new();

// Analyze user routing decisions
for (event, destination) in user_routing_history {
    let pattern = extract_pattern(&event);
    patterns.entry(pattern).or_insert(vec![]).push(destination);
}

// Create filters for common patterns
for (pattern, destinations) in patterns {
    if destinations.len() > 10 {
        let most_common = find_most_common(&destinations);
        create_intelligent_filter(
            &pool,
            format!("Auto-route {}", pattern),
            pattern_to_conditions(&pattern),
            FilterAction::Route(most_common)
        ).await?;
    }
}
```

## Performance Considerations

### Model Training

- **Batch Training**: Train models during off-peak hours
- **Incremental Updates**: Use online learning for real-time updates
- **Resource Limits**: Set CPU/memory limits for training jobs
- **Parallel Training**: Train multiple models concurrently

### Inference

- **Model Caching**: Cache loaded models in memory
- **Batch Prediction**: Process multiple predictions together
- **Async Processing**: Run inference asynchronously
- **Fallback**: Use simple rules if ML service unavailable

### Storage

- **Model Versioning**: Keep multiple model versions
- **Pruning**: Remove old predictions and patterns
- **Compression**: Compress model artifacts
- **Archival**: Archive inactive models

## Monitoring

### Metrics to Track

- **Model Accuracy**: Monitor accuracy over time
- **Prediction Latency**: Time to generate predictions
- **False Positive Rate**: For anomaly detection
- **Training Duration**: Time to train models
- **Resource Usage**: CPU/memory for ML operations

### Alerts

- **Model Drift**: Accuracy degraded significantly
- **Training Failures**: Model training errors
- **High Latency**: Predictions taking too long
- **Resource Exhaustion**: Out of memory/CPU

## Best Practices

### Model Lifecycle

1. **Initial Training**: Start with 30+ days of historical data
2. **Validation**: Split data into train/test sets
3. **Deployment**: Deploy models with monitoring
4. **Monitoring**: Track performance metrics
5. **Retraining**: Retrain periodically or on drift detection

### Data Quality

1. **Cleaning**: Remove outliers and invalid data
2. **Normalization**: Scale features appropriately
3. **Feature Engineering**: Create meaningful derived features
4. **Labeling**: Ensure high-quality training labels

### Security

1. **Model Privacy**: Don't leak training data
2. **Adversarial Defense**: Protect against adversarial inputs
3. **Access Control**: Restrict model management APIs
4. **Audit Logging**: Log all ML operations

## Limitations

### Current Limitations

- No deep learning models (neural networks)
- Limited to supervised and unsupervised learning
- Requires minimum training data (100+ samples)
- English-only for text features
- No GPU acceleration

### Future Enhancements

- [ ] Deep learning model support
- [ ] Reinforcement learning for optimization
- [ ] Natural language processing
- [ ] Computer vision for chart analysis
- [ ] AutoML for automatic model selection
- [ ] Federated learning for privacy
- [ ] GPU acceleration
- [ ] Pre-trained models marketplace

## Troubleshooting

### Model Training Fails

**Problem**: Insufficient training data

**Solution**: 
```sql
SELECT COUNT(*) FROM events WHERE contract_id = 'CABC...' AND created_at >= NOW() - INTERVAL '30 days';
```

Ensure at least 100 samples exist.

### Low Prediction Accuracy

**Problem**: Model not capturing patterns

**Solution**:
- Increase training data window
- Add more relevant features
- Adjust hyperparameters
- Try different model types

### High False Positive Rate

**Problem**: Too many anomaly alerts

**Solution**:
- Increase anomaly threshold
- Improve baseline statistics
- Add contextual features
- Use ensemble methods

## References

- [Time Series Forecasting](https://otexts.com/fpp3/)
- [Anomaly Detection Techniques](https://en.wikipedia.org/wiki/Anomaly_detection)
- [Dynamic Time Warping](https://en.wikipedia.org/wiki/Dynamic_time_warping)
- [Naive Bayes Classifier](https://en.wikipedia.org/wiki/Naive_Bayes_classifier)

## See Also

- [Anomaly Detection Documentation](ANOMALY_DETECTION.md)
- [Event Processing](EVENT_PROCESSING.md)
- [API Documentation](API.md)
- [Performance Tuning](PERFORMANCE.md)
