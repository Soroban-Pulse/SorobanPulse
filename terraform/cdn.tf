# CloudFront CDN Configuration for SorobanPulse

resource "aws_cloudfront_distribution" "soroban_pulse" {
  enabled             = true
  is_ipv6_enabled     = true
  comment             = "SorobanPulse API CDN"
  default_root_object = ""
  price_class         = "PriceClass_All"

  origin {
    domain_name = var.origin_domain
    origin_id   = "soroban-pulse-origin"

    custom_origin_config {
      http_port              = 80
      https_port             = 443
      origin_protocol_policy = "https-only"
      origin_ssl_protocols   = ["TLSv1.2"]
    }

    custom_header {
      name  = "X-Origin-Verify"
      value = var.origin_verify_secret
    }
  }

  default_cache_behavior {
    allowed_methods  = ["DELETE", "GET", "HEAD", "OPTIONS", "PATCH", "POST", "PUT"]
    cached_methods   = ["GET", "HEAD", "OPTIONS"]
    target_origin_id = "soroban-pulse-origin"

    forwarded_values {
      query_string = true
      headers      = ["Authorization", "Accept", "Content-Type"]

      cookies {
        forward = "none"
      }
    }

    viewer_protocol_policy = "redirect-to-https"
    min_ttl                = 0
    default_ttl            = 300
    max_ttl                = 3600
    compress               = true
  }

  # Cache behavior for ledger data
  ordered_cache_behavior {
    path_pattern     = "/api/v1/ledgers/*"
    allowed_methods  = ["GET", "HEAD", "OPTIONS"]
    cached_methods   = ["GET", "HEAD"]
    target_origin_id = "soroban-pulse-origin"

    forwarded_values {
      query_string = true
      headers      = ["Accept"]
      cookies {
        forward = "none"
      }
    }

    viewer_protocol_policy = "redirect-to-https"
    min_ttl                = 60
    default_ttl            = 300
    max_ttl                = 600
    compress               = true
  }

  # Cache behavior for transaction data
  ordered_cache_behavior {
    path_pattern     = "/api/v1/transactions/*"
    allowed_methods  = ["GET", "HEAD", "OPTIONS"]
    cached_methods   = ["GET", "HEAD"]
    target_origin_id = "soroban-pulse-origin"

    forwarded_values {
      query_string = true
      headers      = ["Accept"]
      cookies {
        forward = "none"
      }
    }

    viewer_protocol_policy = "redirect-to-https"
    min_ttl                = 60
    default_ttl            = 600
    max_ttl                = 1200
    compress               = true
  }

  # No cache for streaming endpoints
  ordered_cache_behavior {
    path_pattern     = "/api/v1/events/stream*"
    allowed_methods  = ["GET", "HEAD"]
    cached_methods   = ["GET", "HEAD"]
    target_origin_id = "soroban-pulse-origin"

    forwarded_values {
      query_string = true
      headers      = ["*"]
      cookies {
        forward = "all"
      }
    }

    viewer_protocol_policy = "redirect-to-https"
    min_ttl                = 0
    default_ttl            = 0
    max_ttl                = 0
  }

  restrictions {
    geo_restriction {
      restriction_type = "none"
    }
  }

  viewer_certificate {
    acm_certificate_arn      = var.acm_certificate_arn
    ssl_support_method       = "sni-only"
    minimum_protocol_version = "TLSv1.2_2021"
  }

  tags = {
    Name        = "soroban-pulse-cdn"
    Environment = var.environment
    ManagedBy   = "terraform"
  }
}

# CloudFront Origin Access Identity
resource "aws_cloudfront_origin_access_identity" "soroban_pulse" {
  comment = "SorobanPulse CDN OAI"
}

# Route53 alias for CDN
resource "aws_route53_record" "cdn" {
  zone_id = var.route53_zone_id
  name    = var.cdn_domain_name
  type    = "A"

  alias {
    name                   = aws_cloudfront_distribution.soroban_pulse.domain_name
    zone_id                = aws_cloudfront_distribution.soroban_pulse.hosted_zone_id
    evaluate_target_health = false
  }
}

output "cloudfront_distribution_id" {
  value       = aws_cloudfront_distribution.soroban_pulse.id
  description = "CloudFront distribution ID"
}

output "cloudfront_domain_name" {
  value       = aws_cloudfront_distribution.soroban_pulse.domain_name
  description = "CloudFront distribution domain name"
}
